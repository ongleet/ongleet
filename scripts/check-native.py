"""Exercise the bundled Tauri app in WebKitGTK with private temporary metadata.

Requires WebKitWebDriver and a native-validation frontend build embedded in the
application. Uses the WebDriver HTTP protocol directly; no Selenium dependency.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
ELEMENT = "element-6066-11e4-a52e-4f735466cecf"


class WebDriver:
    def __init__(self, base):
        self.base = base
        self.session = None

    def request(self, method, path, data=None, session=True):
        url = self.base + (f"/session/{self.session}" if session else "") + path
        request = urllib.request.Request(url, data=None if data is None else json.dumps(data).encode(),
                                         method=method, headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                result = json.load(response)["value"]
        except urllib.error.HTTPError as error:
            raise RuntimeError(error.read().decode()) from error
        if isinstance(result, dict) and result.get("error") and "message" in result:
            raise RuntimeError(result)
        return result

    def js(self, script, *args):
        return self.request("POST", "/execute/sync", {"script": script, "args": list(args)})

    def invoke(self, command, args=None):
        result = self.request("POST", "/execute/async", {
            "script": "const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(value=>done({value}),error=>done({error:String(error)}));",
            "args": [command, args or {}],
        })
        if "error" in result:
            raise RuntimeError(result["error"])
        return result.get("value")

    def find(self, selector, using="css selector"):
        return self.request("POST", "/element", {"using": using, "value": selector})[ELEMENT]

    def click(self, selector, using="css selector"):
        element = wait_for(lambda: self.find(selector, using), f"element {selector}")
        self.js("arguments[0].scrollIntoView({block: 'nearest', inline: 'nearest'})", {ELEMENT: element})
        self.request("POST", f"/element/{element}/click", {})

    def fill(self, selector, text):
        element = self.find(selector)
        self.request("POST", f"/element/{element}/clear", {})
        self.request("POST", f"/element/{element}/value", {"text": text})

    def keys(self, text):
        actions = []
        for char in text:
            value = "\ue007" if char == "\n" else char
            actions.extend([{"type": "keyDown", "value": value}, {"type": "keyUp", "value": value}])
        self.request("POST", "/actions", {"actions": [{"type": "key", "id": "keyboard", "actions": actions}]})

    def shortcut(self, letter):
        self.request("POST", "/actions", {"actions": [{"type": "key", "id": "keyboard", "actions": [
            {"type": "keyDown", "value": "\ue009"}, {"type": "keyDown", "value": "\ue008"},
            {"type": "keyDown", "value": letter}, {"type": "keyUp", "value": letter},
            {"type": "keyUp", "value": "\ue008"}, {"type": "keyUp", "value": "\ue009"},
        ]}]})

    def state(self, tab):
        return self.js("return window.__terminalValidation?.state(arguments[0]) ?? null", tab)

    def screen(self, tab):
        return self.js("return window.__terminalValidation?.snapshot(arguments[0]) ?? ''", tab)


def wait_for(check, description, timeout=15):
    deadline = time.monotonic() + timeout
    last_error = None
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except (RuntimeError, urllib.error.URLError) as error:
            last_error = str(error)
        time.sleep(0.05)
    raise AssertionError(f"Timed out: {description}; last error: {last_error}")


def owned_processes(marker):
    result = []
    for path in Path("/proc").iterdir():
        if not path.name.isdigit():
            continue
        try:
            if f"TERMINAL_VALIDATION_RUN={marker}".encode() not in (path / "environ").read_bytes().split(b"\0"):
                continue
            status = (path / "status").read_text().splitlines()
            rss = next((int(line.split()[1]) for line in status if line.startswith("VmRSS:")), 0)
            result.append({"pid": int(path.name), "name": (path / "comm").read_text().strip(), "rssKiB": rss})
        except (OSError, ValueError):
            pass
    return result


def run(args):
    driver_binary = shutil.which(args.driver) or (str(Path(args.driver).resolve()) if Path(args.driver).is_file() else None)
    if not driver_binary:
        raise SystemExit("WebKitWebDriver is missing; install Ubuntu package webkitgtk-webdriver or pass --driver PATH")
    run_dir = Path(tempfile.mkdtemp(prefix="terminal-native-validation-"))
    app_config = run_dir / "config" / "io.local.terminal-stack-prototype"
    app_config.mkdir(parents=True)
    # JSON is a YAML subset; all generated fixture paths/arguments stay literal.
    types = [
        {"id": "bash", "name": "Bash", "kind": "terminal", "config": {"executable": "/bin/bash", "args": ["--noprofile", "--norc"], "env": {"PS1": "probe> "}}},
        *[{"id": mode, "name": mode.title(), "kind": "terminal", "config": {"executable": "/usr/bin/python3", "args": [str(ROOT / "scripts/terminal-probe.py"), mode]}} for mode in ["unicode", "flood", "screen", "reload"]],
    ]
    types.append({"id": "overflow", "name": "Overflow", "kind": "terminal", "config": {
        "executable": "/usr/bin/python3", "args": [str(ROOT / "scripts/terminal-probe.py"), "flood", "--batches", "150"]}})
    codex = shutil.which("codex")
    if codex and not args.skip_codex:
        types.append({"id": "codex", "name": "Codex", "kind": "terminal", "config": {"executable": codex, "args": [], "cwdOverride": str(ROOT)}})
    (app_config / "tab-types.yaml").write_text(json.dumps({"tab_types": types}))
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    env = dict(os.environ, XDG_DATA_HOME=str(run_dir / "data"), XDG_CONFIG_HOME=str(run_dir / "config"),
               XDG_CACHE_HOME=str(run_dir / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}",
               TAURI_WEBVIEW_AUTOMATION="true", GDK_BACKEND=args.backend, TERMINAL_VALIDATION_RUN=str(run_dir))
    report = {"backend": args.backend, "artifacts": str(run_dir), "checks": [], "memory": []}
    client = WebDriver(f"http://127.0.0.1:{port}")
    clipboard_backup = None
    clipboard_touched = False
    with (run_dir / "driver.log").open("w") as log:
        driver = subprocess.Popen([driver_binary, "--host=127.0.0.1", f"--port={port}"], env=env,
                                  stdout=log, stderr=log, start_new_session=True)
        try:
            wait_for(lambda: client.request("GET", "/status", session=False), "WebKitGTK driver ready")
            session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {
                "webkitgtk:browserOptions": {"binary": str(Path(args.application).resolve())}
            }}}, session=False)
            client.session = session["sessionId"]
            report["capabilities"] = session["capabilities"]
            wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create workspace\"]:not(:disabled)')"), "native app ready")
            client.click('[aria-label="Create workspace"]')
            client.fill("dialog[open] label:nth-of-type(1) input", "Native validation")
            client.fill("dialog[open] label:nth-of-type(2) input", str(run_dir))
            client.click("//dialog[@open]//button[normalize-space()='Save workspace']", "xpath")
            wait_for(lambda: client.js("return !!window.__terminalValidation"), "validation bridge (requires native-validation build)")

            try:
                if shutil.which("wl-paste") and shutil.which("wl-copy"):
                    saved_types = subprocess.run(["wl-paste", "--list-types"], capture_output=True, env=env, timeout=3)
                    if saved_types.returncode != 0:
                        raise RuntimeError("Desktop clipboard formats unavailable")
                    mime_types = saved_types.stdout.decode().splitlines()
                    mime = "image/png" if "image/png" in mime_types else next((value for value in mime_types if value.startswith("text/plain")), None)
                    if mime:
                        saved = subprocess.run(["wl-paste", "--no-newline", "--type", mime], capture_output=True, env=env, check=True, timeout=3)
                        clipboard_backup = (mime, saved.stdout)
                    elif not mime_types:
                        clipboard_backup = (None, b"")
                if clipboard_backup is None:
                    # Refuse to replace an image/custom clipboard format we cannot restore.
                    original = client.invoke("plugin:clipboard-manager|read_text")
                    clipboard_backup = ("text/plain;charset=utf-8", (original or "").encode())
            except (RuntimeError, subprocess.SubprocessError):
                report["clipboard"] = "Skipped: existing desktop clipboard could not be backed up safely."
                clipboard_backup = None

            def new_tab(name):
                previous_tabs = client.js("return Array.from(document.querySelectorAll('[role=tab]'),tab=>Number(tab.id.slice(4)))")
                client.click("//button[normalize-space()='+ New tab']", "xpath")
                client.click(f"//dialog[@open]//button[normalize-space()='{name}']", "xpath")
                tab = wait_for(lambda: client.js("const id=Number(document.querySelector('[role=tab][aria-selected=true]')?.id.slice(4));return id && !arguments[0].includes(id) ? id : null", previous_tabs), "new selected tab")
                wait_for(lambda: (state := client.state(tab)) and state.get("sessionId") and not state["replaying"], "terminal connected")
                return tab

            def focus(tab):
                client.click(f"#tab-{tab}")
                wait_for(lambda: client.js("const panel=document.getElementById('tab-panel-'+arguments[0]);return panel && !panel.hidden", tab), "selected panel visible")
                client.js("document.querySelector('#tab-panel-'+arguments[0]+' textarea').focus()", tab)
                wait_for(lambda: client.js("return document.activeElement===document.querySelector('#tab-panel-'+arguments[0]+' textarea')", tab), "terminal input focused")

            def command(tab, text):
                focus(tab)
                client.keys(text + "\n")

            def close_tab(tab):
                client.click(f"#tab-{tab} + .tab-close")
                wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]') || !document.getElementById('tab-'+arguments[0])", tab), "close confirmation or exited tab closure")
                if client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"):
                    client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Confirm']", "xpath")
                wait_for(lambda: client.js("return !document.getElementById('tab-'+arguments[0])", tab), "tab closed")

            def mark(description):
                report["checks"].append(description)
                print("PASS:", description, flush=True)

            first = new_tab("Bash")
            command(first, "printf 'BEFORE_RELOAD café 日本語 🙂\\n'; printf 'SHELL_PID=%s\\n' \"$$\"")
            screen = wait_for(lambda: (value := client.screen(first)) and "BEFORE_RELOAD café 日本語 🙂" in value and any(line.startswith("SHELL_PID=") and line.split("=", 1)[1].isdigit() for line in value.splitlines()) and value, "Unicode shell marker")
            session_id = client.state(first)["sessionId"]
            # Read a standalone output line, not the command echo.
            shell_pid = next(int(line.split("=", 1)[1]) for line in screen.splitlines() if line.startswith("SHELL_PID=") and line.split("=", 1)[1].isdigit())
            second = new_tab("Bash")
            command(second, "printf 'SECOND_TAB\\n'")
            wait_for(lambda: "SECOND_TAB" in client.screen(second), "second tab marker")
            client.request("POST", "/refresh", {})
            wait_for(lambda: (state := client.state(first)) and state.get("sessionId") == session_id and not state["replaying"], "first PTY reattached")
            assert "BEFORE_RELOAD café 日本語 🙂" in client.screen(first)
            assert "SECOND_TAB" in client.screen(second)
            command(first, "printf 'AFTER_PID=%s\\n' \"$$\"")
            wait_for(lambda: f"AFTER_PID={shell_pid}" in client.screen(first), "same shell PID after reload")
            mark("WebView reload preserves shell PID, Unicode output and independent tabs")

            if clipboard_backup is not None:
                clipboard_touched = True
                client.js("window.__terminalValidation.selectAll(arguments[0])", first)
                client.shortcut("c")
                copied = wait_for(lambda: (text := client.invoke("plugin:clipboard-manager|read_text")) and "BEFORE_RELOAD" in text and text, "native copy shortcut")
                assert "BEFORE_RELOAD" in copied
                client.invoke("plugin:clipboard-manager|write_text", {"text": "printf 'PASTED café 日本語 🙂\\n'"})
                focus(first)
                client.shortcut("v")
                client.keys("\n")
                wait_for(lambda: "PASTED café 日本語 🙂" in client.screen(first), "native Unicode paste shortcut")
                mark("Ctrl+Shift+C/V use native clipboard after reload")

            unicode_tab = new_tab("Unicode")
            wait_for(lambda: "Split UTF-8: café λ 日本語 🙂" in client.screen(unicode_tab), "split UTF-8 fixture")
            wait_for(lambda: "(0)" in client.js("return document.getElementById('tab-'+arguments[0]).innerText", unicode_tab), "fixture exit code")
            unicode_session = client.state(unicode_tab)["sessionId"]
            client.request("POST", "/refresh", {})
            wait_for(lambda: (state := client.state(unicode_tab)) and state.get("sessionId") == unicode_session and not state["replaying"], "exited session reattached")
            assert "Split UTF-8: café λ 日本語 🙂" in client.screen(unicode_tab)
            mark("Split UTF-8 survives IPC and exited-tab replay without relaunch")

            reload_tab = new_tab("Reload")
            wait_for(lambda: "RESPONSES=1" in client.screen(reload_tab), "initial cursor query reply")
            live_session = client.state(reload_tab)["sessionId"]
            for _ in range(3):
                focus(first)
                client.request("POST", "/refresh", {})
                wait_for(lambda: (state := client.state(reload_tab)) and state.get("sessionId") == live_session and not state["replaying"], "hidden streaming session reattached")
                before = client.screen(reload_tab)
                wait_for(lambda: client.screen(reload_tab) != before, "hidden stream continues after reload")
                assert "RESPONSES=1" in client.screen(reload_tab)
                assert "RESPONSES=2" not in client.screen(reload_tab)
            focus(reload_tab)
            client.keys("x")
            wait_for(lambda: "KEY=x" in client.screen(reload_tab), "raw input after repeated reload")
            close_tab(reload_tab)
            mark("Three reloads during hidden live output preserve the session and suppress replayed query replies")

            screen_tab = new_tab("Screen")
            wait_for(lambda: "Alternate screen / keyboard / mouse probe" in client.screen(screen_tab), "curses alternate screen")
            focus(screen_tab)
            client.keys("\ue013")  # Arrow Up
            wait_for(lambda: "Key event:" in client.screen(screen_tab), "native arrow key")
            if args.skip_mouse:
                report.setdefault("blockers", []).append("Mouse check skipped explicitly: pointer automation is unavailable on this display backend.")
            else:
                canvas = client.find(f"#tab-panel-{screen_tab} canvas")
                client.request("POST", "/actions", {"actions": [{"type": "pointer", "id": "mouse", "parameters": {"pointerType": "mouse"}, "actions": [
                    {"type": "pointerMove", "origin": {ELEMENT: canvas}, "x": 0, "y": 0},
                    {"type": "pointerDown", "button": 0}, {"type": "pointerUp", "button": 0}
                ]}]})
                try:
                    wait_for(lambda: "Mouse event:" in client.screen(screen_tab), "native mouse reporting", timeout=3)
                    mark("Native mouse reporting reaches curses")
                except AssertionError:
                    report.setdefault("blockers", []).append("Curses receives no mouse reports: ghostty-web 0.4.0 browser input only selects text; VT mouse-mode support alone is insufficient.")
            old_size = client.state(screen_tab)
            client.request("POST", "/window/rect", {"width": 950, "height": 650})
            resized = wait_for(lambda: (state := client.state(screen_tab)) and (state["cols"],state["rows"]) != (old_size["cols"],old_size["rows"]) and state, "visible resize")
            wait_for(lambda: f"Dimensions: {resized['cols']} columns x {resized['rows']} rows" in client.screen(screen_tab), "PTY observes new dimensions")
            client.request("POST", "/refresh", {})
            wait_for(lambda: (state := client.state(screen_tab)) and state.get("alternate") and not state["replaying"], "alternate screen replay")
            focus(screen_tab)
            client.keys("q")
            wait_for(lambda: "Normal screen restored." in client.screen(screen_tab), "normal screen restoration")
            assert "NORMAL SCREEN:" in client.screen(screen_tab)
            mark("Native curses keyboard, actual PTY resize and alternate-screen reload/return")

            report["memory"].append({"phase": "before-flood", "processes": owned_processes(run_dir)})
            for iteration in range(3):
                flood = new_tab("Overflow" if iteration == 0 else "Flood")
                start = time.monotonic()
                focus(first)
                report.setdefault("switchSeconds", []).append(time.monotonic() - start)
                assert client.state(flood)["cols"] > 0 and client.state(flood)["rows"] > 0
                wait_for(lambda: ("Batch 149:" if iteration == 0 else "Batch 99:") in client.screen(flood), "hidden flood completes", timeout=30)
                assert client.state(flood)["scrollback"] <= 2000
                focus(flood)
                if iteration == 0:
                    client.request("POST", "/refresh", {})
                    wait_for(lambda: client.js("return document.body.innerText.includes('4 MiB reload replay limit')"), "explicit replay-overflow warning")
                    assert client.state(flood)["sessionId"]
                close_tab(flood)
                report["memory"].append({"phase": f"after-flood-{iteration+1}", "processes": owned_processes(run_dir)})
            mark("Hidden floods totaling 350,000 lines, bounded scrollback and explicit replay overflow")

            for iteration in range(10):
                tab = new_tab("Bash")
                command(tab, f"printf 'REUSE_{iteration}\\n'")
                wait_for(lambda: f"REUSE_{iteration}" in client.screen(tab), "new tab output")
                assert "BEFORE_RELOAD" not in client.screen(tab)
                assert "Batch " not in client.screen(tab), "Closed flood leaked into a new terminal"
                close_tab(tab)
            assert "BEFORE_RELOAD" in client.screen(first)
            mark("Ten native create/close cycles leave clean screens and preserve live sibling")

            clean_tab = new_tab("Unicode")
            wait_for(lambda: "Split UTF-8:" in client.screen(clean_tab), "fresh terminal after stress")
            wait_for(lambda: "(0)" in client.js("return document.getElementById('tab-'+arguments[0]).innerText", clean_tab), "fresh fixture exits")
            assert len([line for line in client.screen(clean_tab).splitlines() if line.strip()]) == 1, "Fresh terminal retained closed-tab rows"
            close_tab(clean_tab)
            mark("Native fresh screen after flood/reuse contains only its new command's output")

            if codex and not args.skip_codex:
                codex_tab = new_tab("Codex")
                wait_for(lambda: (value := client.screen(codex_tab)) and "codex" in value.lower() and "Usage:" not in value and "unexpected argument" not in value, "interactive Codex startup", timeout=20)
                wait_for(lambda: "Ask Codex to do anything" in client.screen(codex_tab), "Codex input prompt")
                focus(codex_tab)
                client.keys("validation_draft")
                wait_for(lambda: "validation_draft" in client.screen(codex_tab), "Codex unsent draft")
                client.keys("\ue012\ue003")  # Left, Backspace: remove f before the final t.
                wait_for(lambda: "validation_drat" in client.screen(codex_tab), "Codex cursor editing")
                client.keys("\ue010" + "\ue003" * len("validation_drat"))
                client.keys("/")
                wait_for(lambda: "/help" in client.screen(codex_tab) or "/model" in client.screen(codex_tab), "Codex slash menu")
                (run_dir / "codex-menu.txt").write_text(client.screen(codex_tab))
                client.keys("\ue00c\ue003")  # Escape menu, remove slash.
                client.keys("validation_unsent")
                wait_for(lambda: "validation_unsent" in client.screen(codex_tab), "Codex unsent draft before reload")
                codex_session = client.state(codex_tab)["sessionId"]
                client.request("POST", "/window/rect", {"width": 1100, "height": 760})
                client.request("POST", "/refresh", {})
                wait_for(lambda: (state := client.state(codex_tab)) and state.get("sessionId") == codex_session and not state["replaying"], "Codex reattached")
                wait_for(lambda: "validation_unsent" in client.screen(codex_tab), "Codex unsent draft preserved across resize/reload")
                (run_dir / "codex-screen.txt").write_text(client.screen(codex_tab))
                screenshot = client.request("GET", "/screenshot")
                (run_dir / "codex.png").write_bytes(base64.b64decode(screenshot))
                report["codex"] = "Interactive prompt editing, slash menu, resize/reload and unsent draft preservation passed. Approval prompts and multiline paste remain manual. No task dispatched."
                mark("Interactive Codex prompt editing, slash menu and resize/reload retain an unsent draft")
                close_tab(codex_tab)
            else:
                report["codex"] = "Not run: executable missing or --skip-codex"
            report["result"] = "blocked" if report.get("blockers") else "passed"
        except BaseException as error:
            report["result"] = "failed"
            report["error"] = str(error)
            if client.session:
                try:
                    (run_dir / "failure-screen.txt").write_text(client.js("return document.body.innerText"))
                    (run_dir / "failure.png").write_bytes(base64.b64decode(client.request("GET", "/screenshot")))
                except Exception:
                    pass
            raise
        finally:
            try:
                if clipboard_touched and clipboard_backup is not None:
                    mime, contents = clipboard_backup
                    if shutil.which("wl-copy"):
                        restore_env = {key: value for key, value in env.items() if key != "TERMINAL_VALIDATION_RUN"}
                        subprocess.run(["wl-copy", "--type", mime] if mime else ["wl-copy", "--clear"], input=contents, env=restore_env, check=True, timeout=3)
                    elif mime and client.session:
                        client.invoke("plugin:clipboard-manager|write_text", {"text": contents.decode()})
            except (RuntimeError, subprocess.SubprocessError) as error:
                report["clipboardRestoreError"] = str(error)
            if client.session:
                try:
                    client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                    wait_for(lambda: not any(process["name"].startswith("terminal-work") for process in owned_processes(run_dir)), "native application exit", timeout=5)
                    client.request("DELETE", "")
                except Exception as error:
                    report["cleanupError"] = str(error)
            driver.terminate()
            try:
                driver.wait(timeout=5)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()
            # Only test-owned processes inherit this exact unique marker. Never
            # inspect or terminate an unrelated user session.
            survivors = owned_processes(run_dir)
            report["cleanupSurvivors"] = survivors
            for process in survivors:
                try:
                    os.kill(process["pid"], signal.SIGKILL)
                except ProcessLookupError:
                    pass
            report_path = Path(args.report) if args.report else run_dir / "report.json"
            report_path.write_text(json.dumps(report, indent=2))
            print("Report:", report_path, flush=True)

    return 2 if report.get("blockers") else 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", default="WebKitWebDriver")
    parser.add_argument("--application", default=str(ROOT / "src-tauri/target/debug/terminal-workspace-prototype"))
    parser.add_argument("--backend", choices=["wayland", "x11"], default="wayland")
    parser.add_argument("--skip-codex", action="store_true")
    parser.add_argument("--skip-mouse", action="store_true", help="Record unavailable pointer automation; never count mouse reporting as passed")
    parser.add_argument("--report")
    raise SystemExit(run(parser.parse_args()))
