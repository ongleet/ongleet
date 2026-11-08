#!/usr/bin/env python3
"""Smoke-test a staged executable-only deployment before publishing its ZIP."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile


EXECUTABLES = ("ongleet-gui.exe", "ongleet.exe", "ong-codex.exe")


def smoke_test(directory: Path, isolated: Path) -> None:
    # Keep the runner's runtime/toolchain environment, but never target a live
    # instance or inherit a terminal's implicit session context.
    env = {key: value for key, value in os.environ.items()
           if not key.upper().startswith("ONGLEET_")}
    env.update({
        "XDG_DATA_HOME": str(isolated / "data"),
        "XDG_CONFIG_HOME": str(isolated / "config"),
        "LOCALAPPDATA": str(isolated / "local-app-data"),
        "APPDATA": str(isolated / "app-data"),
    })

    def run(name: str, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [str(directory / name), *args], cwd=isolated, env=env,
            capture_output=True, text=True, encoding="utf-8", errors="strict",
            timeout=35, check=False,
        )

    for name in ("ongleet.exe", "ong-codex.exe"):
        result = run(name, "--help")
        if result.returncode != 0 or name.removesuffix(".exe") not in result.stdout:
            raise RuntimeError(f"Staged {name} help failed: {result.returncode}\n"
                               f"{result.stdout}\n{result.stderr}")

    result = run("ongleet.exe", "--json", "app", "status")
    if result.returncode != 3:
        raise RuntimeError(f"Expected unavailable exit 3, got {result.returncode}\n"
                           f"{result.stdout}\n{result.stderr}")
    response = json.loads(result.stderr)
    if response.get("ok") is not False or response.get("error", {}).get("code") != "unavailable":
        raise RuntimeError(f"Expected structured unavailable response: {response!r}")
    print("Staged CLI/Codex help and isolated unavailable-instance checks passed")


def package(binary_directory: Path, output: Path) -> None:
    # Spaces and Unicode in the deployment path exercise executable resolution.
    with tempfile.TemporaryDirectory(prefix="ongleet-artifact-") as temporary:
        isolated = Path(temporary).resolve()
        stage = isolated / "Ongleet café deployment"
        stage.mkdir()
        for name in EXECUTABLES:
            source = binary_directory / name
            if not source.is_file() or source.is_symlink():
                raise RuntimeError(f"Missing regular release executable: {source}")
            shutil.copy2(source, stage / name)
        smoke_test(stage, isolated)
        output.parent.mkdir(parents=True, exist_ok=True)
        # Keep a failed packaging operation from publishing a partial ZIP.
        with tempfile.TemporaryDirectory(dir=output.parent) as archive_directory:
            archive_path = Path(archive_directory) / output.name
            with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as archive:
                for name in EXECUTABLES:
                    archive.write(stage / name, arcname=name)
            with zipfile.ZipFile(archive_path) as archive:
                if tuple(archive.namelist()) != EXECUTABLES or archive.testzip() is not None:
                    raise RuntimeError("Executable ZIP contents or integrity check failed")
            os.replace(archive_path, output)
    print(f"Created {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-directory", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    options = parser.parse_args()
    package(options.binary_directory.resolve(), options.output.resolve())
