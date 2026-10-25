"""Validate live Codex approvals, task interruption and scrolling in native GTK.

Uses private application/Codex metadata and a temporary copy of file-based auth.
Only fixture prompts are submitted. The private Codex home is removed on exit.
"""
import argparse
import base64
import importlib.util
import json
import os
import re
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import tomllib

spec = importlib.util.spec_from_file_location("native", Path(__file__).with_name("check-native.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def run(args):
    source_home = Path(args.auth_home)
    if not (source_home / "auth.json").is_file():
        raise SystemExit("File-based Codex authentication is required; personal configuration will not be changed")
    codex = shutil.which("codex")
    if not codex:
        raise SystemExit("Codex is missing from PATH")
    directory = Path(tempfile.mkdtemp(prefix="devcenter-codex-validation-"))
    workspace = directory / "workspace"
    workspace.mkdir()
    private_home = directory / "codex-home"
    private_home.mkdir(mode=0o700)
    model = tomllib.loads((source_home / "config.toml").read_text()).get("model")
    config = 'cli_auth_credentials_store = "file"\nmodel_reasoning_effort = "low"\n'
    if model:
        config += f"model = {json.dumps(model)}\n"
    config += f'\n[projects.{json.dumps(str(workspace))}]\ntrust_level = "trusted"\n'
    (private_home / "config.toml").write_text(config)
    fixture = workspace / "probe.py"
    fixture.write_text('''import os, pathlib, signal, sys, time
root = pathlib.Path(__file__).parent
mode = sys.argv[1]
if mode == "approve":
    (root / "approved").write_text("approved")
    print("APPROVAL_FIXTURE_DONE", flush=True)
elif mode == "interrupt":
    (root / "active-pid").write_text(str(os.getpid()))
    def stop(*_):
        raise SystemExit(0)
    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    try:
        print("INTERRUPT_FIXTURE_STARTED", flush=True)
        for index in range(180):
            print(f"fixture waiting {index}", flush=True)
            time.sleep(1)
    finally:
        (root / "stopped").write_text("stopped")
''')
    config_dir = directory / "config/devcenter/projects/1"
    config_dir.mkdir(parents=True)
    (config_dir / "actions.yaml").write_text(json.dumps({"actions": [{
        "id": "codex", "name": "Live Codex", "kind": "terminal", "config": {
            "executable": codex, "args": ["--no-daemon", "--sandbox", "read-only", "--ask-for-approval", "on-request"],
            "cwdOverride": str(workspace), "env": {"CODEX_HOME": str(private_home)},
        }}]}))
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"), XDG_CONFIG_HOME=str(directory / "config"),
               XDG_CACHE_HOME=str(directory / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}",
               TAURI_WEBVIEW_AUTOMATION="true", GDK_BACKEND=args.backend, TERMINAL_VALIDATION_RUN=str(directory))
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    client = native.WebDriver(f"http://127.0.0.1:{port}")
    report = {"backend": args.backend, "artifacts": str(directory), "checks": [],
              "codexVersion": subprocess.check_output([codex, "--version"], text=True).strip(), "model": model}

    def mark(description):
        report["checks"].append(description)
        print("PASS:", description, flush=True)

    def capture(name):
        (directory / f"{name}.txt").write_text(client.screen(tab))
        (directory / f"{name}.png").write_bytes(base64.b64decode(client.request("GET", "/screenshot")))

    def focus():
        assert client.js("const input=document.querySelector('#tab-panel-'+arguments[0]+' textarea');input.focus();return document.activeElement===input", tab)

    def ctrl(letter):
        client.request("POST", "/actions", {"actions": [{"type": "key", "id": "keyboard", "actions": [
            {"type": "keyDown", "value": "\ue009"}, {"type": "keyDown", "value": letter},
            {"type": "keyUp", "value": letter}, {"type": "keyUp", "value": "\ue009"},
        ]}]})

    def prompt(text):
        focus()
        client.keys(text)
        native.wait_for(lambda: text[-32:].replace(" ", "") in client.screen(tab).replace(" ", "").replace("\n", ""), "fixture draft delivered")
        client.keys("\n")

    def approval():
        def pending():
            value = client.screen(tab)
            (directory / "last-screen.txt").write_text(value)
            return "Would you like" in value or "approve this" in value.lower() or "Yes, proceed" in value
        native.wait_for(pending, "interactive approval prompt", timeout=90)

    def turn_done():
        return any(line.lstrip("• ").strip() == "APPROVAL_TURN_DONE" for line in client.screen(tab).splitlines())

    def alive(pid):
        try:
            return not Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].lstrip().startswith("Z")
        except FileNotFoundError:
            return False

    driver = None
    tab = None
    try:
        shutil.copy2(source_home / "auth.json", private_home / "auth.json")
        with (directory / "driver.log").open("w") as log:
            driver = subprocess.Popen([args.driver, "--host=127.0.0.1", f"--port={port}"], env=env,
                                      stdout=log, stderr=log, start_new_session=True)
        native.wait_for(lambda: client.request("GET", "/status", session=False), "WebDriver ready")
        session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "webkitgtk:browserOptions": {"binary": str(Path(args.application).resolve())}
        }}}, session=False)
        client.session = session["sessionId"]
        native.wait_for(lambda: client.find('[aria-label="Create session"]'), "application ready")
        # Session-form typing is covered elsewhere. Seed this fixture through
        # the normal backend operation so setup does not depend on native text
        # entry, then reload the frontend from persisted state.
        client.invoke("create_session", {"name": "Codex acceptance", "directory": str(workspace), "projectId": 1})
        client.request("POST", "/refresh", {})
        tab = native.wait_for(lambda: client.js("return Number(document.querySelector('[role=tab][aria-selected=true]')?.id.slice(4)) || null"), "Codex tab created")
        native.wait_for(lambda: "Ask Codex to do anything" in client.screen(tab), "authenticated Codex prompt", timeout=45)
        mark("Private authenticated Codex starts in an isolated workspace without a shared daemon")
        task = (f"This is a terminal acceptance fixture. Do not inspect files or run any other command. "
                f"Use exec_command once with cmd='/usr/bin/python3 {fixture} approve', sandbox_permissions='require_escalated', "
                "justification='Approve the isolated terminal fixture?'. After its result reply only APPROVAL_TURN_DONE.")
        prompt(task)
        approval()
        assert not (workspace / "approved").exists(), "Command ran before approval"
        capture("approval-pending")
        focus()
        client.keys("\ue00c")  # Escape rejects this execution.
        native.wait_for(lambda: turn_done() or "interrupted" in client.screen(tab).lower(), "approval rejection handled", timeout=45)
        assert not (workspace / "approved").exists(), "Rejected command ran"
        mark("Live approval prompt blocks execution; rejection leaves the fixture untouched")
        prompt(task + " Ask for approval again; the prior rejection was intentional.")
        approval()
        focus()
        client.keys("y")
        native.wait_for(lambda: (workspace / "approved").exists(), "approved command executes", timeout=45)
        native.wait_for(turn_done, "approved turn finishes", timeout=45)
        capture("approval-accepted")
        mark("Approving a live Codex request executes only the isolated marker command")
        prompt(f"Terminal fixture: run only '/usr/bin/python3 {fixture} interrupt' with exec_command, sandbox_permissions='require_escalated', justification='Run interrupt fixture?'. Keep waiting for it until interrupted; do not run any other command.")
        approval()
        focus()
        client.keys("y")
        native.wait_for(lambda: (workspace / "active-pid").exists(), "active fixture process", timeout=45)
        active_pid = int((workspace / "active-pid").read_text())
        assert alive(active_pid), "Fixture exited before interruption"
        capture("active-task")
        focus()
        ctrl("c")
        native.wait_for(lambda: not alive(active_pid), "Ctrl+C stops the active fixture process", timeout=20)
        native.wait_for(lambda: "conversationinterrupted" in client.screen(tab)[-700:].replace(" ", "").lower(), "current Codex turn reports interruption", timeout=20)
        capture("interrupted")
        mark("Native Ctrl+C interrupts the active Codex turn and stops its fixture process")
        prompt("Do not use tools. Reply with exactly eighty plain lines, no bullets or code fences. Each line is SCROLL_ followed by its three-digit line number, a space, and terminal validation. Start at one and end at eighty.")
        native.wait_for(lambda: "SCROLL_080" in client.screen(tab), "long Codex response completes", timeout=90)
        native.wait_for(lambda: "Working(" not in client.screen(tab).replace(" ", ""), "long response turn settles")
        capture("scroll-bottom")
        before_scroll = client.screen(tab)
        before_rows = [int(value) for value in re.findall(r"SCROLL_(\d{3})", before_scroll)]
        before_viewport = client.state(tab).get("viewportY")
        canvas = client.find(f"#tab-panel-{tab} canvas")
        def wheel(delta):
            client.request("POST", "/actions", {"actions": [{"type": "wheel", "id": "scroll", "actions": [{
                "type": "scroll", "origin": {native.ELEMENT: canvas}, "x": 0, "y": 0,
                "deltaX": 0, "deltaY": delta, "duration": 250,
            }]}]})
        wheel(-600)
        def scrolled():
            rows = [int(value) for value in re.findall(r"SCROLL_(\d{3})", client.screen(tab))]
            return client.state(tab).get("viewportY") != before_viewport or (rows and max(rows) < max(before_rows) - 2)
        native.wait_for(scrolled, "Codex history scrolls upward")
        capture("scroll-up")
        report["scroll"] = {"beforeViewport": before_viewport, "afterViewport": client.state(tab).get("viewportY"),
                            "beforeRows": before_rows, "scrolledRows": [int(value) for value in re.findall(r"SCROLL_(\d{3})", client.screen(tab))]}
        wheel(3000)
        native.wait_for(lambda: client.state(tab).get("viewportY") == before_viewport and "SCROLL_080" in client.screen(tab), "Codex scroll returns to latest response")
        capture("scroll-return")
        mark("Native wheel scrolls Codex history upward and returns to the latest response")
        report["result"] = "passed"
    except BaseException as error:
        report["result"] = "failed"
        report["error"] = str(error)
        if client.session:
            try:
                if tab:
                    capture("failure")
                else:
                    (directory / "failure.txt").write_text(client.js("return document.body.innerText"))
                    (directory / "failure.png").write_bytes(base64.b64decode(client.request("GET", "/screenshot")))
            except Exception:
                pass
        raise
    finally:
        if client.session:
            try:
                client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                native.wait_for(lambda: not any(p["name"] == "devcenter-gui" for p in native.owned_processes(directory)), "test GUI exits", timeout=5)
                client.request("DELETE", "")
            except Exception as error:
                report["cleanupError"] = str(error)
        if driver:
            driver.terminate()
            try:
                driver.wait(timeout=5)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()
        survivors = native.owned_processes(directory)
        report["cleanupSurvivors"] = survivors
        for process in survivors:
            try:
                os.kill(process["pid"], signal.SIGKILL)
            except ProcessLookupError:
                pass
        shutil.rmtree(private_home)
        (directory / "report.json").write_text(json.dumps(report, indent=2))
        print("Report:", directory / "report.json", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["wayland", "x11"], default="wayland")
    parser.add_argument("--driver", default="WebKitWebDriver")
    parser.add_argument("--application", default=str(native.ROOT / "src-tauri/target/debug/devcenter-gui"))
    parser.add_argument("--auth-home", default=os.environ.get("CODEX_HOME", str(Path.home() / ".codex")))
    run(parser.parse_args())
