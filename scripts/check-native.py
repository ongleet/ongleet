"""Exercise the bundled Tauri app in WebKitGTK with private temporary metadata.

Requires WebKitWebDriver and a native-validation frontend build embedded in the
application. Uses the WebDriver HTTP protocol directly; no Selenium dependency.
"""
import argparse
import base64
import json
import os
import re
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
        wait_for(lambda: self.request("GET", f"/element/{element}/enabled"), f"enabled element {selector}")
        self.js("arguments[0].scrollIntoView({block: 'nearest', inline: 'nearest'})", {ELEMENT: element})
        self.request("POST", f"/element/{element}/click", {})

    def fill(self, selector, text):
        element = self.find(selector)
        self.js("arguments[0].scrollIntoView({block:'nearest',inline:'nearest'})", {ELEMENT: element})
        self.request("POST", f"/element/{element}/clear", {})
        if "\n" in text and self.request("GET", f"/element/{element}/name") == "textarea":
            self.js("arguments[0].focus()", {ELEMENT: element})
            self.keys(text)
        else:
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


def wayland_minimize(client):
    """Activate the focused button with compositor input, not a GTK simulation.

    WebDriver's synthetic keys have no Wayland serial. GTK needs a real serial
    when requesting an activation token later. Use a temporary GNOME session;
    no desktop settings, clipboard or persistent input services are changed.
    """
    from gi.repository import Gio, GLib
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    destination = "org.gnome.Mutter.RemoteDesktop"
    interface = destination + ".Session"
    path = connection.call_sync(destination, "/org/gnome/Mutter/RemoteDesktop", destination,
                                "CreateSession", None, None, Gio.DBusCallFlags.NONE, 3000, None).unpack()[0]
    started = False
    pressed = False
    def call(method, parameters=None):
        return connection.call_sync(destination, path, interface, method, parameters,
                                    None, Gio.DBusCallFlags.NONE, 3000, None)
    try:
        call("Start")
        started = True
        assert client.js("const button=document.querySelector('[aria-label=\"Minimize window\"]'); button.focus(); return document.activeElement===button"), "Minimize button did not receive focus"
        assert client.invoke("plugin:window|is_focused", {"label": "main"}), "Test window lost focus before native input"
        call("NotifyKeyboardKeycode", GLib.Variant("(ub)", (28, True)))  # Linux KEY_ENTER.
        pressed = True
        call("NotifyKeyboardKeycode", GLib.Variant("(ub)", (28, False)))
        pressed = False
    finally:
        try:
            if pressed:
                call("NotifyKeyboardKeycode", GLib.Variant("(ub)", (28, False)))
        finally:
            if started:
                call("Stop")


def run(args):
    driver_binary = shutil.which(args.driver) or (str(Path(args.driver).resolve()) if Path(args.driver).is_file() else None)
    if not driver_binary:
        raise SystemExit("WebKitWebDriver is missing; install Ubuntu package webkitgtk-webdriver or pass --driver PATH")
    run_dir = Path(tempfile.mkdtemp(prefix="terminal-native-validation-"))
    app_config = run_dir / "config" / "devcenter"
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
    (app_config / "projects/1").mkdir(parents=True)
    (app_config / "projects/1/actions.yaml").write_text(json.dumps({"actions": types}))
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    env = dict(os.environ, XDG_DATA_HOME=str(run_dir / "data"), XDG_CONFIG_HOME=str(run_dir / "config"),
               XDG_CACHE_HOME=str(run_dir / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}",
               TAURI_WEBVIEW_AUTOMATION="true", GDK_BACKEND=args.backend, TERMINAL_VALIDATION_RUN=str(run_dir))
    driver_env = dict(env)
    if args.check_minimize and args.backend == "wayland":
        # xdg-shell exposes the request, but no minimized state notification.
        driver_env.update(DEVCENTER_DEBUG="1", WAYLAND_DEBUG="client")
    report = {"backend": args.backend, "artifacts": str(run_dir), "checks": [], "memory": []}
    # A private URI handler records link activation without opening the user's browser.
    applications = run_dir / "data/applications"
    applications.mkdir(parents=True)
    link_log = run_dir / "opened-links.txt"
    handler = run_dir / "open-link.py"
    handler.write_text("import pathlib,sys\nwith pathlib.Path(" + repr(str(link_log)) + ").open('a') as log: log.write(sys.argv[1]+'\\n')\n")
    (applications / "devcenter-validation.desktop").write_text(
        "[Desktop Entry]\nType=Application\nName=DevCenter validation\nExec=/usr/bin/python3 " + str(handler) + " %u\nMimeType=x-scheme-handler/https;\n")
    (run_dir / "config/mimeapps.list").write_text("[Default Applications]\nx-scheme-handler/https=devcenter-validation.desktop\n")
    uri_handler = subprocess.run(["gio", "mime", "x-scheme-handler/https"], env=env, capture_output=True, text=True, check=True)
    assert "devcenter-validation.desktop" in uri_handler.stdout, "Test URI handler must be registered before clicking links"
    client = WebDriver(f"http://127.0.0.1:{port}")
    clipboard_backup = None
    clipboard_touched = False
    with (run_dir / "driver.log").open("w") as log:
        driver = subprocess.Popen([driver_binary, "--host=127.0.0.1", f"--port={port}"], env=driver_env,
                                  stdout=log, stderr=log, start_new_session=True)
        try:
            wait_for(lambda: client.request("GET", "/status", session=False), "WebKitGTK driver ready")
            session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {
                "webkitgtk:browserOptions": {"binary": str(Path(args.application).resolve())}
            }}}, session=False)
            client.session = session["sessionId"]
            report["capabilities"] = session["capabilities"]
            wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), "native app ready")
            client.click('[aria-label="Create session"]')
            client.fill("#session-name", "Native validation")
            client.fill("#session-directory", str(run_dir))
            client.click("//dialog[@open]//button[normalize-space()='Save session']", "xpath")
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
                client.click('[aria-label="Session menu"]')
                client.click(f"//details[@open]//button[@title='Open a new tab'][normalize-space()='{name}']", "xpath")
                tab = wait_for(lambda: client.js("const id=Number(document.querySelector('[role=tab][aria-selected=true]')?.id.slice(4));return id && !arguments[0].includes(id) ? id : null", previous_tabs), "new selected tab")
                wait_for(lambda: (state := client.state(tab)) and state.get("sessionId") and not state["replaying"], "terminal connected")
                return tab

            def focus(tab):
                # WebKitGTK's pointer endpoint rejects some horizontally clipped
                # tabs after resizing. Exercise the native button keyboard path.
                wait_for(lambda: client.js("const button=document.getElementById('tab-'+arguments[0]); if (!button || button.disabled) return false; button.focus(); return document.activeElement===button", tab), "tab button focused")
                client.keys("\ue007")  # Enter activates the focused tab button.
                wait_for(lambda: client.js("const panel=document.getElementById('tab-panel-'+arguments[0]);return panel && !panel.hidden", tab), "selected panel visible")
                wait_for(lambda: client.js("const input=document.querySelector('#tab-panel-'+arguments[0]+' textarea'); input?.focus(); return document.activeElement===input", tab), "terminal input focused")

            def command(tab, text):
                focus(tab)
                client.keys(text + "\n")

            def close_tab(tab):
                wait_for(lambda: client.js("const button=document.querySelector('#tab-'+arguments[0]+' + .tab-close'); if (!button || button.disabled) return false; button.focus(); return document.activeElement===button", tab), "close button focused")
                client.keys("\ue007")
                wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]') || !document.getElementById('tab-'+arguments[0])", tab), "close confirmation or exited tab closure")
                if client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"):
                    client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Confirm']", "xpath")
                wait_for(lambda: client.js("return !document.getElementById('tab-'+arguments[0])", tab), "tab closed")

            def mark(description):
                report["checks"].append(description)
                print("PASS:", description, flush=True)

            client.click('[aria-label="Maximize window"]')
            wait_for(lambda: client.find('[aria-label="Restore window"]'), "native maximize state")
            client.click('[aria-label="Restore window"]')
            wait_for(lambda: client.find('[aria-label="Maximize window"]'), "native restore state")
            mark("Custom title bar maximize/restore follows native window state")

            first = new_tab("Bash")
            command(first, "printf 'BEFORE_RELOAD café 日本語 🙂\\n'; printf 'SHELL_PID=%s\\n' \"$$\"")
            screen = wait_for(lambda: (value := client.screen(first)) and "BEFORE_RELOAD café 日本語 🙂" in value and any(line.startswith("SHELL_PID=") and line.split("=", 1)[1].isdigit() for line in value.splitlines()) and value, "Unicode shell marker")
            session_id = client.state(first)["sessionId"]
            # Read a standalone output line, not the command echo.
            shell_pid = next(int(line.split("=", 1)[1]) for line in screen.splitlines() if line.startswith("SHELL_PID=") and line.split("=", 1)[1].isdigit())
            if args.check_minimize:
                for maximized in (False, True):
                    if maximized:
                        client.click('[aria-label="Maximize window"]')
                        wait_for(lambda: client.find('[aria-label="Restore window"]'), "maximized before minimize")
                    before_rect = client.request("GET", "/window/rect")
                    wait_for(lambda: client.invoke("plugin:window|is_focused", {"label": "main"}), "native window focused before minimize")
                    trace_offset = (run_dir / "driver.log").stat().st_size
                    if args.backend == "wayland":
                        wayland_minimize(client)
                        def minimize_request():
                            with (run_dir / "driver.log").open() as trace:
                                trace.seek(trace_offset)
                                lines = trace.read().splitlines()
                            return next((line for line in lines if re.search(r"-> xdg_toplevel[@#]\d+\.set_minimized\(\)", line)), None)
                        request = wait_for(minimize_request, "Wayland xdg_toplevel minimize request")
                        wait_for(lambda: not client.invoke("plugin:window|is_focused", {"label": "main"}), "Wayland focus lost after minimize request")
                        report.setdefault("minimize", []).append({"request": request, "focusLost": True, "maximized": maximized, "minimizedState": "Unavailable in xdg-shell"})
                    else:
                        client.click('[aria-label="Minimize window"]')
                        wait_for(lambda: client.invoke("plugin:window|is_minimized", {"label": "main"}), "native minimized state")
                    subprocess.run([args.application, "--show"], env=env, check=True, timeout=5)
                    wait_for(lambda: client.invoke("plugin:window|is_focused", {"label": "main"}), "native window focused after --show")
                    if args.backend == "wayland":
                        report["minimize"][-1]["focusRestored"] = True
                        mark("Wayland minimize request loses focus; --show reactivates the native window")
                    else:
                        wait_for(lambda: not client.invoke("plugin:window|is_minimized", {"label": "main"}), "native window restored")
                        mark("Custom minimize and --show restore the native window")
                    label = "Restore window" if maximized else "Maximize window"
                    wait_for(lambda: client.find(f'[aria-label="{label}"]'), "maximization preserved after activation")
                    wait_for(lambda: all(client.request("GET", "/window/rect")[key] == before_rect[key] for key in ["width", "height"]), "window dimensions preserved after activation")
                client.click('[aria-label="Restore window"]')
                wait_for(lambda: client.find('[aria-label="Maximize window"]'), "normal window restored after checks")
                mark("Minimize/reactivation preserves normal and maximized window dimensions")
            # WebDriver's click endpoint can wait forever for a repaint after
            # this control hides the WebView. Dispatch after the JS reply.
            client.js("setTimeout(()=>document.querySelector('[aria-label=\"Close window\"]').click(),0)")
            wait_for(lambda: not client.invoke("application_status")["visible"], "custom close hides window")
            subprocess.run([args.application, "--show"], env=env, check=True, timeout=5)
            wait_for(lambda: client.invoke("application_status")["visible"], "hidden window reopened")
            command(first, "printf 'WINDOW_PID=%s\\n' \"$$\"")
            wait_for(lambda: f"WINDOW_PID={shell_pid}" in client.screen(first).splitlines(), "same shell after minimize/hide")
            mark("Custom close-to-hide and --show preserve the live shell PID")
            if args.window_only:
                report["result"] = "passed"
                return 0
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
                wait_for(lambda: "printf 'PASTED café 日本語 🙂\\n'" in client.screen(first), "pasted command reaches shell before Enter")
                client.keys("\n")
                wait_for(lambda: "PASTED café 日本語 🙂" in client.screen(first).splitlines(), "native Unicode paste shortcut")
                mark("Ctrl+Shift+C/V use native clipboard after reload")
                command(first, "printf '\\033[2J\\033[HSELECT_WITH_MOUSE\\n'")
                wait_for(lambda: client.screen(first).splitlines()[0] == "SELECT_WITH_MOUSE", "selection fixture")
                rect = client.js("const r=document.querySelector('#tab-panel-'+arguments[0]+' canvas').getBoundingClientRect(); const s=window.__terminalValidation.state(arguments[0]);return {x:r.x,y:r.y,w:r.width/s.cols,h:r.height/s.rows}", first)
                client.request("POST", "/actions", {"actions": [{"type":"pointer", "id":"selection", "parameters":{"pointerType":"mouse"}, "actions":[
                    {"type":"pointerMove", "origin":"viewport", "x":round(rect['x']+1), "y":round(rect['y']+rect['h']/2)},
                    {"type":"pointerDown", "button":0},
                    {"type":"pointerMove", "origin":"viewport", "x":round(rect['x']+16*rect['w']), "y":round(rect['y']+rect['h']/2), "duration":250},
                    {"type":"pointerUp", "button":0}
                ]}]})
                wait_for(lambda: "SELECT_WITH_MOUSE" in (client.invoke("plugin:clipboard-manager|read_text") or ""), "mouse selection copies to native clipboard")
                mark("Real mouse drag selects terminal text and copies through the native clipboard")
                if shutil.which("wl-copy"):
                    subprocess.run(["wl-copy", "--type", "image/png"], input=(ROOT / "src-tauri/icons/32x32.png").read_bytes(), env=env, check=True, timeout=3)
                    wait_for(lambda: client.invoke("terminal_clipboard") == {"kind":"image"}, "image clipboard ownership")
                    subprocess.run(["wl-copy", "--type", "text/uri-list"], input=b"file:///tmp/a%20b.png\r\nfile:///tmp/it%27s.png\r\n", env=env, check=True, timeout=3)
                    wait_for(lambda: client.invoke("terminal_clipboard") == {"kind":"text", "text":"'/tmp/a b.png' '/tmp/it'\\''s.png'"}, "file clipboard ownership")
                    mark("Native clipboard distinguishes images and copied files, preserving spaces and quotes")

            command(first, "printf '\\033[2J\\033[H\\033]8;;https://example.invalid/devcenter-input-test\\007OPEN_TEST\\033]8;;\\007\\nBEFORE_RELOAD\\n'")
            wait_for(lambda: client.screen(first).splitlines()[0] == "OPEN_TEST", "OSC 8 link fixture")
            rect = client.js("const r=document.querySelector('#tab-panel-'+arguments[0]+' canvas').getBoundingClientRect();return {x:r.x+20,y:r.y+8}", first)
            client.request("POST", "/actions", {"actions": [
                {"type":"key", "id":"keyboard", "actions":[{"type":"keyDown", "value":"\ue009"}]},
                {"type":"pointer", "id":"links", "parameters":{"pointerType":"mouse"}, "actions":[
                    {"type":"pointerMove", "origin":"viewport", "x":round(rect['x']), "y":round(rect['y'])},
                    {"type":"pointerDown", "button":0}, {"type":"pointerUp", "button":0}]}
            ]})
            client.request("POST", "/actions", {"actions":[{"type":"key", "id":"keyboard", "actions":[{"type":"keyUp", "value":"\ue009"}]}]})
            wait_for(lambda: link_log.exists() and "https://example.invalid/devcenter-input-test" in link_log.read_text(), "native OSC 8 Ctrl-click activation")
            mark("Ctrl-click on an OSC 8 terminal link opens the native registered URI handler")

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
            for key, expected in [("\ue031", 265), ("\ue03c", 276)]:  # F1, F12; ncurses KEY_F(1/12).
                client.keys(key)
                wait_for(lambda: f"Key event: {expected}" in client.screen(screen_tab), f"native function key {expected}")
            mark("Native F1 and F12 reach curses as function keys")
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
                    report.setdefault("blockers", []).append("Native pointer automation produced no observed curses mouse report; investigate input delivery on this backend.")
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
                if clipboard_backup is not None:
                    clipboard_touched = True
                    pasted = "validation_line_one café\nvalidation_line_two 日本語 🙂"
                    client.invoke("plugin:clipboard-manager|write_text", {"text": pasted})
                    client.shortcut("v")
                    def multiline_draft():
                        value = client.screen(codex_tab)
                        (run_dir / "codex-multiline.txt").write_text(value)
                        # Codex paints some blank cells without literal spaces;
                        # the text snapshot omits them. Keep the screenshot too.
                        return all(token in value for token in ["validation_line_one", "café", "validation_line_two", "日本語", "🙂"])
                    wait_for(multiline_draft, "Codex multiline clipboard draft")
                    (run_dir / "codex-multiline.txt").write_text(client.screen(codex_tab))
                    (run_dir / "codex-multiline.png").write_bytes(base64.b64decode(client.request("GET", "/screenshot")))
                    client.request("POST", "/actions", {"actions": [{"type": "key", "id": "keyboard", "actions": [
                        {"type": "keyDown", "value": "\ue009"}, {"type": "keyDown", "value": "c"},
                        {"type": "keyUp", "value": "c"}, {"type": "keyUp", "value": "\ue009"},
                    ]}]})
                    wait_for(lambda: "validation_line_one" not in client.screen(codex_tab) and "validation_line_two" not in client.screen(codex_tab), "Ctrl+C clears Codex multiline draft")
                    mark("Codex native multiline Unicode paste remains editable; Ctrl+C clears the unsent draft")
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
                report["codex"] = "Interactive prompt editing, slash menu, resize/reload and unsent draft preservation passed. Multiline paste/draft Ctrl+C ran only when clipboard backup was available. Approval prompts, active-task interruption and scrolling remain manual. No task dispatched."
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
                    wait_for(lambda: not any(process["name"] == "devcenter-gui" for process in owned_processes(run_dir)), "native application exit", timeout=5)
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
    parser.add_argument("--application", default=str(ROOT / "src-tauri/target/debug/devcenter-gui"))
    parser.add_argument("--backend", choices=["wayland", "x11"], default="wayland")
    parser.add_argument("--skip-codex", action="store_true")
    parser.add_argument("--check-minimize", action="store_true", help="Exercise compositor-sensitive native minimize/restore")
    parser.add_argument("--window-only", action="store_true", help="Run window controls and shell PID preservation without the terminal stress suite")
    parser.add_argument("--skip-mouse", action="store_true", help="Record unavailable pointer automation; never count mouse reporting as passed")
    parser.add_argument("--report")
    raise SystemExit(run(parser.parse_args()))
