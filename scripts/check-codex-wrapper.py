#!/usr/bin/env python3
"""Exercise the real wrapper/hook subprocess boundary without an API call."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


binary = Path(sys.argv[1] if len(sys.argv) > 1 else
              "src-tauri/target/debug/dcodex").resolve()
with tempfile.TemporaryDirectory(prefix="codex-wrapper-'quoted-") as temp:
    root = Path(temp)
    wrapper = root / "wrapper with spaces'"
    shutil.copy2(binary, wrapper)
    fake = root / "codex"
    fake.write_text('''#!/usr/bin/python3
import json, os, subprocess, sys, tomllib
args = sys.argv[1:]
settings = {}
for index, arg in enumerate(args):
    if arg == "-c":
        layer = tomllib.loads(args[index + 1])
        if "hooks" in layer:
            settings.setdefault("hooks", {}).update(layer["hooks"])
command = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
event = {"session_id": os.environ["TEST_CONVERSATION"], "hook_event_name": "SessionStart"}
subprocess.run(command, shell=True, input=json.dumps(event), text=True, check=True)
print(json.dumps({"args": args, "cwd": os.getcwd(), "env": os.environ["TEST_INHERITED"]}))
sys.exit(23)
''')
    fake.chmod(0o700)
    env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
               DEVCENTER_CODEX_DATA=str(root / "data"), TEST_INHERITED="literal $value")

    def launch(session, tab, conversation, *options):
        result = subprocess.run([str(wrapper), "--session-id", str(session),
                                 "--tab-id", str(tab), *options],
                                env=dict(env, TEST_CONVERSATION=conversation),
                                cwd=root, text=True, capture_output=True, timeout=10)
        assert result.returncode == 23, result.stderr
        output = json.loads(result.stdout)
        assert output["cwd"] == str(root)
        assert output["env"] == "literal $value"
        return output["args"]

    first = launch(12, 34, "first-id")
    assert "resume" not in first
    assert "--no-daemon" in first and "--dangerously-bypass-hook-trust" in first
    resumed = launch(12, 34, "changed-id", "--", "--model", "example")
    assert resumed[-4:] == ["resume", "first-id", "--model", "example"], resumed
    assert "changed-id" in launch(12, 34, "changed-id")
    assert "resume" not in launch(12, 35, "sibling-id")
    assert "resume" not in launch(13, 34, "other-session-id")
    assert "resume" not in launch(12, 34, "fresh-id", "--fresh")
    path = root / "data/codex-sessions/12/34.json"
    assert json.loads(path.read_text())["conversation"] == "fresh-id"
    assert path.stat().st_mode & 0o777 == 0o600
    path.write_text("invalid state")
    rejected = subprocess.run([str(wrapper), "--session-id", "12", "--tab-id", "34"],
                              env=env, text=True, capture_output=True, timeout=10)
    assert rejected.returncode == 1 and "Invalid Codex session file" in rejected.stderr
    assert path.read_text() == "invalid state"
print("Codex wrapper subprocess checks passed (stand-in Codex; no API call).")
