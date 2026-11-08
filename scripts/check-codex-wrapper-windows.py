#!/usr/bin/env python3
"""Native Windows wrapper/shim/hook tests with a stand-in Codex; no API calls."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def check(wrapper: Path) -> None:
    if sys.platform != "win32":
        raise RuntimeError("This fixture requires native Windows")
    with tempfile.TemporaryDirectory(prefix="ongleet-codex-") as temporary:
        root = Path(temporary) / "café 日本語 and 'quotes'"
        root.mkdir()
        executable = root / "ong-codex with 'quotes'.exe"
        shutil.copy2(wrapper, executable)
        (root / "codex.cmd").write_text(
            '@"%TEST_PYTHON%" "%~dp0standin.py" %*\r\n', encoding="ascii"
        )
        (root / "standin.py").write_text('''
import json, os, subprocess, sys, tomllib
args = sys.argv[1:]
settings = {}
for index, arg in enumerate(args):
    if arg == '-c':
        layer = tomllib.loads(args[index+1])
        if 'hooks' in layer:
            settings.setdefault('hooks', {}).update(layer['hooks'])
command = settings['hooks']['SessionStart'][0]['hooks'][0]['command']
with open(os.environ['TEST_ATTEMPTS'], 'a', encoding='utf-8') as output:
    output.write(json.dumps(args) + '\\n')
event = {'session_id': 'windows-conversation', 'hook_event_name': 'SessionStart',
         'unknown_tool_payload': 'x' * (3 * 1024 * 1024)}
powershell = os.path.join(os.environ['SystemRoot'], 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
result = subprocess.run([powershell, '-NoLogo', '-NoProfile', '-NonInteractive', '-Command', command],
                        input=json.dumps(event), text=True, capture_output=True, timeout=15)
if result.returncode:
    sys.stderr.write(result.stderr)
    sys.exit(91)
sys.exit(23)
''', encoding="utf-8")
        env = {key: value for key, value in os.environ.items()
               if not key.upper().startswith("ONGLEET_")}
        env.update(TEST_PYTHON=sys.executable, PATH=str(root) + os.pathsep + env["PATH"],
                   ONGLEET_CODEX_DATA=str(root / "data"),
                   XDG_DATA_HOME=str(root / "instances"),
                   XDG_CONFIG_HOME=str(root / "config"))

        def launch(session: int, tab: int, *options: str, restart: bool = False):
            attempts = root / f"attempts-{session}-{tab}.jsonl"
            attempts.unlink(missing_ok=True)
            result = subprocess.run(
                [str(executable), "--session-id", str(session), "--tab-id", str(tab),
                 *([] if restart else ["--no-restart"]), *options],
                cwd=root, env=dict(env, TEST_ATTEMPTS=str(attempts)),
                capture_output=True, timeout=45,
            )
            expected = 1 if restart else 23
            if result.returncode != expected:
                raise RuntimeError(f"Wrapper exited {result.returncode}, expected {expected}: "
                                   f"{result.stdout!r}\n{result.stderr!r}")
            calls = [json.loads(line) for line in attempts.read_text(encoding="utf-8").splitlines()]
            saved = json.loads((root / "data" / "codex-sessions" / str(session) / f"{tab}.json").read_text(encoding="utf-8"))
            assert saved["conversation"] == "windows-conversation", saved
            return calls, result

        calls, _ = launch(1, 2, "--", "--model", "literal space 日本語 & % !")
        assert "resume" not in calls[0]
        assert calls[0][-2:] == ["--model", "literal space 日本語 & % !"]
        calls, _ = launch(1, 2)
        assert calls[0][-2:] == ["resume", "windows-conversation"]
        calls, _ = launch(1, 2, "--fresh")
        assert "resume" not in calls[0]
        calls, result = launch(3, 4, restart=True)
        assert len(calls) == 5, calls
        assert "resume" not in calls[0]
        assert all(call[-2:] == ["resume", "windows-conversation"] for call in calls[1:])
        assert b"too quickly five times" in result.stderr
    print("Windows Codex shim, quoted hooks, 3 MiB payload, recording, resume and restart checks passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wrapper", required=True, type=Path)
    check(parser.parse_args().wrapper.resolve())
