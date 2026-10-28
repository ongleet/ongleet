"""Clipboard checks on a private Xvfb display; never uses the desktop clipboard."""
import argparse
import base64
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import socket
import struct
import subprocess
import tempfile

spec = importlib.util.spec_from_file_location("native", Path(__file__).with_name("check-native.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class Selection:
    def __init__(self, primary, env):
        if env.get("ONGLEET_PRIVATE_X11") != "1" or env.get("GDK_BACKEND") != "x11":
            raise RuntimeError("Clipboard validation requires its private X11 display")
        self.primary, self.env, self.touched = primary, env, False
        self.flags = ["-selection", "primary" if primary else "clipboard"]
        result = subprocess.run(["xclip", *self.flags, "-o", "-t", "TARGETS"], env=env, capture_output=True, timeout=3)
        if result.returncode:
            if b"target TARGETS not available" not in result.stderr:
                raise RuntimeError("Selection formats unavailable; cannot safely back up")
            self.original = {}
            return
        types = result.stdout.decode().splitlines()
        metadata = {"TARGETS", "TIMESTAMP", "MULTIPLE", "SAVE_TARGETS"}
        types = [value for value in types if value not in metadata]
        if not types or len(types) > 32:
            raise RuntimeError("Selection formats cannot safely be restored")
        self.original = {}
        total = 0
        for mime in types:
            data = self.read(mime)
            total += len(data)
            if total > 16 * 1024 * 1024:
                raise RuntimeError("Selection backup exceeds the 16 MiB limit")
            self.original[mime] = data

    def read(self, mime="UTF8_STRING"):
        result = subprocess.run(["xclip", *self.flags, "-o", "-t", mime], env=self.env,
                                capture_output=True, timeout=3)
        if result.returncode:
            raise RuntimeError("Selection text unavailable")
        return result.stdout

    def write(self, text):
        self.touched = True
        self.own({"UTF8_STRING": text.encode(), "text/plain;charset=utf-8": text.encode()}, self.env)
        native.wait_for(lambda: self.read() == text.encode(), "test selection ownership")

    def own(self, formats, env):
        owner = subprocess.Popen(["/usr/bin/python3", str(Path(__file__).with_name("restore-selection.py")), *(["--primary"] if self.primary else [])],
                                 env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
        payload = json.dumps({mime: base64.b64encode(data).decode() for mime, data in formats.items()}).encode()
        owner.stdin.write(payload)
        owner.stdin.close()
        if not select.select([owner.stdout], [], [], 3)[0] or owner.stdout.readline() != b"READY\n":
            raise RuntimeError("Multi-format selection ownership failed")
        owner.stdout.close()

    def restore(self):
        if not self.touched:
            return
        # The restored selection owner must outlive this test's process cleanup.
        env = {key: value for key, value in self.env.items() if key != "TERMINAL_VALIDATION_RUN"}
        if not self.original:
            subprocess.run(["xclip", *self.flags, "-i"], input=b"", env=env, check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=3)
            return
        self.own(self.original, env)
        for mime, original in self.original.items():
            native.wait_for(lambda: self.read(mime) == original, "original selection format restored")


def run(args):
    if not args.xvfb:
        raise SystemExit("Xvfb is required; pass --xvfb PATH. Desktop clipboard testing is disabled.")
    directory = Path(tempfile.mkdtemp(prefix="ongleet-clipboard-validation-"))
    received, mode = directory / "received.bin", directory / "mode.json"
    mode.write_text(json.dumps({"version": 0, "bracketed": True, "mouse": False}))
    fixture = directory / "clipboard-probe.py"
    fixture.write_text('''import json, os, pathlib, select, sys, termios, tty
received, mode = map(pathlib.Path, sys.argv[1:])
previous = termios.tcgetattr(0)
version = -1
try:
    tty.setraw(0)
    received.write_bytes(b"")
    with received.open("ab", buffering=0) as sink:
        while True:
            state = json.loads(mode.read_text())
            if state["version"] != version:
                version = state["version"]
                flags = ("\\x1b[?2004h" if state["bracketed"] else "\\x1b[?2004l")
                flags += "\\x1b[?1000h\\x1b[?1006h" if state["mouse"] else "\\x1b[?1000l\\x1b[?1006l"
                os.write(1, (flags + "\\x1b[2J\\x1b[HASCII café e\\u0301\\r\\nWIDE 日本語 🙂\\r\\nTAIL third\\r\\nMODE " + str(version)).encode())
            if select.select([0], [], [], 0.05)[0]:
                data = os.read(0, 8192)
                if not data:
                    break
                sink.write(data)
finally:
    termios.tcsetattr(0, termios.TCSANOW, previous)
''')
    config = directory / "config/ongleet/projects/1"
    config.mkdir(parents=True)
    (config / "actions.yaml").write_text(json.dumps({"actions": [{"id": "clipboard", "name": "Clipboard probe",
        "kind": "terminal", "config": {"executable": "/usr/bin/python3", "args": [str(fixture), str(received), str(mode)]}},
        {"id":"passive","name":"Passive","kind":"terminal","config":{"executable":"/bin/sleep","args":["600"]}}]}))
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"), XDG_CONFIG_HOME=str(directory / "config"),
               XDG_CACHE_HOME=str(directory / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}",
               TAURI_WEBVIEW_AUTOMATION="true", GDK_BACKEND="x11", ONGLEET_PRIVATE_X11="1", TERMINAL_VALIDATION_RUN=str(directory))
    for key in ["WAYLAND_DISPLAY", "WAYLAND_SOCKET", "WAYLAND_DEBUG"]:
        env.pop(key, None)
    display = next(number for number in range(100, 200) if not Path(f"/tmp/.X11-unix/X{number}").exists() and not Path(f"/tmp/.X{number}-lock").exists())
    authority = directory / "Xauthority"
    def field(value):
        return struct.pack(">H", len(value)) + value
    authority.write_bytes(struct.pack(">H", 256) + field(socket.gethostname().encode()) + field(str(display).encode())
                          + field(b"MIT-MAGIC-COOKIE-1") + field(os.urandom(16)))
    env.update(DISPLAY=f":{display}", XAUTHORITY=str(authority))
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    client = native.WebDriver(f"http://127.0.0.1:{port}")
    report = {"backend": "private-x11-Xvfb", "display": env["DISPLAY"], "dataDirectory": env["XDG_DATA_HOME"],
              "waylandDisabled": "WAYLAND_DISPLAY" not in env, "artifacts": str(directory), "checks": [], "skipped": []}
    selections = {}
    driver, server, tab, version = None, None, None, 0

    def mark(text):
        report["checks"].append(text)
        print("PASS:", text, flush=True)

    def focus():
        assert client.js("const input=document.querySelector('#tab-panel-'+arguments[0]+' textarea');input.focus();return document.activeElement===input", tab)

    def set_mode(bracketed, mouse=False):
        nonlocal version
        version += 1
        temporary = mode.with_suffix(".tmp")
        temporary.write_text(json.dumps({"version": version, "bracketed": bracketed, "mouse": mouse}))
        temporary.replace(mode)
        native.wait_for(lambda: f"MODE {version}" in client.screen(tab), "PTY mode change parsed by Ghostty")

    def expect(before, data):
        def exact():
            actual = received.read_bytes()[before:]
            report["lastPasteLengths"] = {"expected": len(data), "received": len(actual)}
            if actual and actual != data:
                report["lastPastePrefix"] = actual[:128].hex()
            return actual == data
        native.wait_for(exact, "exact pasted PTY bytes")

    def point(col, row):
        return client.js("const r=document.querySelector('#tab-panel-'+arguments[0]+' canvas').getBoundingClientRect();const s=window.__terminalValidation.state(arguments[0]);return {x:Math.round(r.x+(arguments[1]+0.2)*r.width/s.cols),y:Math.round(r.y+(arguments[2]+0.5)*r.height/s.rows)}", tab, col, row)

    def pointer(start, end=None, button=0):
        # Each check is a separate drag, not a double/triple-click gesture.
        actions = [{"type": "pause", "duration": 600}, {"type": "pointerMove", "origin": "viewport", **start}, {"type": "pointerDown", "button": button}]
        if end:
            actions.append({"type": "pointerMove", "origin": "viewport", "duration": 250, **end})
        actions.append({"type": "pointerUp", "button": button})
        client.request("POST", "/actions", {"actions": [{"type": "pointer", "id": "selection", "parameters": {"pointerType": "mouse"}, "actions": actions}]})

    try:
        with (directory / "xvfb.log").open("w") as log:
            server = subprocess.Popen([args.xvfb, f":{display}", "-screen", "0", "1280x900x24", "-nolisten", "tcp", "-auth", str(authority)], env=env,
                                      stdout=log, stderr=log, start_new_session=True)
        native.wait_for(lambda: server.poll() is None and Path(f"/tmp/.X11-unix/X{display}").exists(), "private authenticated X11 display")
        # Seed both selections with known fixtures before any application starts.
        # These are the only original contents the backup/restore path can see.
        for primary, data in [(False, {"text/html": b"<b>original fixture</b>", "text/plain;charset=utf-8": b"original fixture"}),
                              (True, {"text/plain;charset=utf-8": b"original primary fixture"})]:
            seed = Selection.__new__(Selection)
            seed.primary, seed.env, seed.touched, seed.original = primary, env, True, data
            seed.flags = ["-selection", "primary" if primary else "clipboard"]
            seed.restore()
        for name, primary in [("clipboard", False), ("primary", True)]:
            try:
                selections[name] = Selection(primary, env)
            except (RuntimeError, subprocess.SubprocessError) as error:
                report["skipped"].append(f"{name}: {error}")
        if "clipboard" not in selections:
            report["result"] = "blocked"
            return 2
        with (directory / "driver.log").open("w") as log:
            driver = subprocess.Popen([args.driver, "--host=127.0.0.1", f"--port={port}"], env=env,
                                      stdout=log, stderr=log, start_new_session=True)
        native.wait_for(lambda: client.request("GET", "/status", session=False), "WebDriver ready")
        session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "webkitgtk:browserOptions": {"binary": str(Path(args.application).resolve())}
        }}}, session=False)
        client.session = session["sessionId"]
        native.wait_for(lambda: client.find('[aria-label="Create session"]'), "application ready")
        client.invoke("create_session", {"name": "Clipboard acceptance", "directory": str(directory), "projectId": 1})
        client.request("POST", "/refresh", {})
        tab = native.wait_for(lambda: client.js("return Number(document.querySelector('[role=tab][aria-selected=true]')?.id.slice(4)) || null"), "clipboard tab")
        native.wait_for(lambda: "MODE 0" in client.screen(tab), "raw PTY fixture ready")
        native.wait_for(lambda: (state := client.state(tab)) and state.get("sessionId") and not state["replaying"], "terminal input attached")
        if args.cli:
            def cli(*arguments):
                return subprocess.run([str(Path(args.cli).resolve()), '--json', *map(str, arguments)], env=env, capture_output=True, text=True, timeout=10)
            text = "literal café 日本語 🙂; $(not-executed)"
            for enter in [False, True]:
                before=received.stat().st_size
                result=cli('tab','input',tab,'--text',text,*(['--enter'] if enter else []))
                assert result.returncode==0,(result.stdout,result.stderr)
                expect(before,text.encode()+(b'\r' if enter else b''))
            mark('Real CLI sends literal Unicode bytes once; Enter is appended only when explicitly requested')
            before=received.stat().st_size
            result=cli('tab','input',tab,'--text','λ'*4096,'--enter')
            assert result.returncode!=0
            assert received.stat().st_size==before,'Rejected oversized CLI input partially reached the PTY'
            mark('CLI rejects UTF-8 input exceeding 8 KiB including Enter without writing a partial payload')
            before=received.stat().st_size
            result=cli('tab','input',999999999,'--text','must not arrive')
            assert result.returncode!=0 and json.loads(result.stderr)['error']['code']=='not_found',(result.stdout,result.stderr)
            assert received.stat().st_size==before
            mark('CLI rejects an unavailable tab ID without writing to a live sibling terminal')
        client.js("""window.pointerEvidence=[];
            for(const type of ['mousedown','mouseup','mousemove']) document.addEventListener(type,e=>{
              if(window.pointerEvidence.length<128)window.pointerEvidence.push({type,button:e.button,buttons:e.buttons,
              x:e.clientX,y:e.clientY,offsetX:e.offsetX,offsetY:e.offsetY,target:e.target.tagName});},true);
            const canvas=document.querySelector('#tab-panel-'+arguments[0]+' canvas');
            for(const type of ['mousedown','mouseup','mousemove'])canvas.addEventListener(type,e=>{
              if(window.pointerEvidence.length<128)window.pointerEvidence.push({type:'after-'+type,
                selection:window.__terminalValidation.state(arguments[0]).selection,range:window.__terminalValidation.state(arguments[0]).selectionRange,offsetX:e.offsetX,offsetY:e.offsetY});});""", tab)
        clipboard = selections["clipboard"]
        payload = "first 'quoted' $HOME; literal\ncafé 日本語 🙂 e\u0301\r\nlast\n"
        normalized = "first 'quoted' $HOME; literal\ncafé 日本語 🙂 e\u0301\nlast\n"
        clipboard.write(payload)
        def clipboard_converted():
            actual = client.invoke("terminal_clipboard")
            report["nativeFixtureConversion"] = actual
            return actual == {"kind": "text", "text": normalized}
        native.wait_for(clipboard_converted, "native clipboard conversion preserves the fixture")
        focus()
        before = received.stat().st_size
        client.shortcut("v")
        expect(before, b"\x1b[200~" + normalized.encode() + b"\x1b[201~")
        mark("GTK normalizes CRLF; multiline Unicode paste reaches the PTY once with bracketed delimiters")
        set_mode(False)
        before = received.stat().st_size
        focus()
        client.shortcut("v")
        expect(before, normalized.encode())
        mark("Unbracketed multiline paste preserves GTK text and adds no Enter")
        set_mode(True)
        clipboard.own({"image/png": (native.ROOT / "src-tauri/icons/32x32.png").read_bytes()}, env)
        native.wait_for(lambda: client.invoke("terminal_clipboard") == {"kind": "image"}, "native image clipboard classification")
        before = received.stat().st_size
        focus()
        client.shortcut("v")
        expect(before, b"\x16")
        mark("Native image clipboard forwards Ctrl+V to the terminal application")
        clipboard.own({"text/uri-list": b"file:///tmp/a%20b.png\r\nfile:///tmp/it%27s.png\r\n"}, env)
        before = received.stat().st_size
        focus()
        client.shortcut("v")
        expect(before, b"\x1b[200~'/tmp/a b.png' '/tmp/it'\\''s.png'\x1b[201~")
        mark("Copied local file URIs paste as quoted paths without shell execution")
        large = "λ" * 32750  # 65,500 UTF-8 bytes, plus twelve delimiter bytes.
        clipboard.write(large)
        before = received.stat().st_size
        focus()
        client.shortcut("v")
        expect(before, b"\x1b[200~" + large.encode() + b"\x1b[201~")
        mark("65,500-byte Unicode clipboard crosses native write chunks without truncation")
        clipboard.write(large + "x")
        before = received.stat().st_size
        focus()
        client.shortcut("v")
        native.wait_for(lambda: "Clipboard exceeds the terminal input limit" in client.js("return document.body.innerText"), "oversized clipboard error")
        assert received.stat().st_size == before, "Oversized paste partially reached the PTY"
        mark("Oversized clipboard text is rejected visibly without partial PTY input")
        if "primary" in selections:
            primary = selections["primary"]
            primary.write("PRIMARY_SENTINEL")
            set_mode(True)
            clipboard.write("FOCUS_SENTINEL")
            pointer(point(1, 0))
            assert clipboard.read() == b"FOCUS_SENTINEL" and primary.read() == b"PRIMARY_SENTINEL"
            mark("A focus click without a drag preserves both selections")
            selected = "ASCII café e\u0301\nWIDE 日本語 🙂\nTAIL third"
            clipboard.write("FORWARD_SELECTION_SENTINEL")
            pointer(point(0, 0), point(9, 2))
            report.setdefault("selectionStages", []).append(client.state(tab))
            native.wait_for(lambda: clipboard.read() == selected.encode(), "forward multiline Unicode selection")
            mark("Forward pointer selection copies multiline combining/wide text without extra spaces")
            clipboard.write("REVERSE_SELECTION_SENTINEL")
            pointer(point(9, 2), point(0, 0))
            report["selectionStages"].append(client.state(tab))
            native.wait_for(lambda: clipboard.read() == selected.encode(), "reverse multiline Unicode selection")
            mark("Reverse pointer selection preserves the same multiline Unicode text")
            clipboard.write("WIDE_SELECTION_SENTINEL")
            pointer(point(5, 1), point(13, 1))
            report["selectionStages"].append(client.state(tab))
            native.wait_for(lambda: clipboard.read() == "日本語 🙂".encode(), "wide character selection boundaries")
            native.wait_for(lambda: primary.read() == "日本語 🙂".encode(), "selected text owns desktop primary selection")
            mark("Wide-character selection also populates desktop primary selection")
            if args.check_navigation:
                retained=client.state(tab)
                client.click('[aria-label="Session and tab menu"]')
                client.click("//details[@open]//button[@title='Open a new tab'][normalize-space()='Passive']",'xpath')
                passive=client.invoke('sessions')['sessions'][0]['selectedTabId']
                native.wait_for(lambda:(state:=client.state(passive)) and state.get('sessionId') and not state['replaying'],'passive sibling attached')
                client.js("document.getElementById('tab-'+arguments[0]).click()",tab)
                native.wait_for(lambda:client.js("return document.getElementById('tab-'+arguments[0]).getAttribute('aria-selected')==='true'",tab),'original tab restored')
                current=client.state(tab)
                assert current['sessionId']==retained['sessionId']
                if current['selection']!=retained['selection'] or current['selectionRange']!=retained['selectionRange']:
                    report.setdefault('navigationFailures',[]).append({'navigation':'tab','before':retained,'after':current})
                else:
                    mark('Terminal selection text/range and runtime survive tab switching')
                pointer(point(5, 1), point(13, 1))
                native.wait_for(lambda:client.state(tab)['selection']=="日本語 🙂",'fresh selection for independent session check')
                retained=client.state(tab)
                # New sessions use a passive process, leaving the raw-byte fixture untouched.
                settings=json.loads((config/'actions.yaml').read_text())
                settings['actions'].reverse()
                (config/'actions.yaml').write_text(json.dumps(settings))
                client.click('[aria-label="Create session"]')
                client.fill('#session-name','Selection sibling')
                client.fill('#session-directory',str(directory))
                client.click("//button[normalize-space()='Save session']",'xpath')
                new_session=native.wait_for(lambda:next((row for row in client.invoke('sessions')['sessions'] if row['name']=='Selection sibling'),None),'sibling session created')
                native.wait_for(lambda:(state:=client.state(new_session['selectedTabId'])) and state.get('sessionId') and not state['replaying'],'sibling session attached')
                client.click("//button[contains(@class,'session-choice')][.//span[normalize-space()='Clipboard acceptance']]",'xpath')
                native.wait_for(lambda:client.js("return document.getElementById('tab-'+arguments[0]).getAttribute('aria-selected')==='true'",tab),'original session restored')
                current=client.state(tab)
                assert current['sessionId']==retained['sessionId']
                if current['selection']!=retained['selection'] or current['selectionRange']!=retained['selectionRange']:
                    report.setdefault('navigationFailures',[]).append({'navigation':'session','before':retained,'after':current})
                else:
                    mark('Terminal selection text/range and runtime survive session switching')
                assert primary.read()=="日本語 🙂".encode()
            primary_payload = "PRIMARY only\n日本語 e\u0301"
            clipboard.write("CLIPBOARD_ONLY")
            primary.write(primary_payload)
            before = received.stat().st_size
            pointer(point(2, 3), button=1)
            expect(before, b"\x1b[200~" + primary_payload.encode() + b"\x1b[201~")
            mark("Middle-click pastes primary selection independently of the regular clipboard")
            set_mode(True, mouse=True)
            before = received.stat().st_size
            pointer(point(2, 3), button=1)
            native.wait_for(lambda: len(re.findall(rb"\x1b\[<1;\d+;\d+[Mm]", received.read_bytes()[before:])) == 2, "middle-button mouse reports")
            assert primary_payload.encode() not in received.read_bytes()[before:]
            mark("Mouse-reporting mode forwards middle-button events without pasting primary text")
        assert not report.get('navigationFailures'), 'Selection retention failed; see navigationFailures in report.json'
        report["result"] = "blocked" if report["skipped"] else "passed"
    except BaseException as error:
        report["result"], report["error"] = "failed", str(error)
        if client.session and tab:
            try:
                (directory / "failure.txt").write_text(client.screen(tab))
                (directory / "failure.png").write_bytes(base64.b64decode(client.request("GET", "/screenshot")))
                report["privateFixtureSelection"] = selections["clipboard"].read().decode(errors="replace")[:256]
                report["pointerEvidence"] = client.js("return window.pointerEvidence")
            except Exception:
                pass
        raise
    finally:
        for name, selection in selections.items():
            try:
                selection.restore()
                report.setdefault("restored", {})[name] = True
            except Exception as error:
                report.setdefault("restoreErrors", {})[name] = str(error)
        if client.session:
            try:
                client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                native.wait_for(lambda: not any(p["name"] == "ongleet-gui" for p in native.owned_processes(directory)), "test GUI exits", timeout=5)
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
        if server:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
        report["cleanupSurvivors"] = native.owned_processes(directory)
        for process in report["cleanupSurvivors"]:
            try:
                os.kill(process["pid"], signal.SIGKILL)
            except ProcessLookupError:
                pass
        (directory / "report.json").write_text(json.dumps(report, indent=2))
        print("Report:", directory / "report.json", flush=True)
    return 2 if report["skipped"] else 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-navigation", action="store_true", help="Also enforce selection retention across tab/session navigation (known failing on Ghostty 0.4.0)")
    parser.add_argument("--cli", help="Also verify exact terminal CLI input bytes using this executable")
    parser.add_argument("--xvfb", default=shutil.which("Xvfb"))
    parser.add_argument("--driver", default="WebKitWebDriver")
    parser.add_argument("--application", default=str(native.ROOT / "src-tauri/target/debug/ongleet-gui"))
    raise SystemExit(run(parser.parse_args()))
