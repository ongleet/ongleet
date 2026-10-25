"""Check native function keys, Unicode and real GTK preedit against PTY bytes."""
import argparse
import base64
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile

from native_input import GnomeKeyboard

spec = importlib.util.spec_from_file_location("native", Path(__file__).with_name("check-native.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def run(args):
    directory = Path(tempfile.mkdtemp(prefix="devcenter-keyboard-validation-"))
    received = directory / "received.bin"
    fixture = directory / "input-probe.py"
    fixture.write_text('''import os, pathlib, sys, termios, tty
path = pathlib.Path(sys.argv[1])
previous = termios.tcgetattr(0)
try:
    tty.setraw(0)
    path.write_bytes(b"")
    os.write(1, "INPUT READY\\r\\nRender: e\\u0301 A\\u030a n\\u0303 | 👩‍💻 | END\\r\\n".encode())
    with path.open("ab", buffering=0) as sink:
        while True:
            data = os.read(0, 8192)
            if not data:
                break
            sink.write(data)
            os.write(1, ("RX " + data.hex() + "\\r\\n").encode())
finally:
    termios.tcsetattr(0, termios.TCSANOW, previous)
''')
    config = directory / "config/devcenter/projects/1"
    config.mkdir(parents=True)
    (config / "actions.yaml").write_text(json.dumps({"actions": [{
        "id": "input", "name": "Input probe", "kind": "terminal", "config": {
            "executable": "/usr/bin/python3", "args": [str(fixture), str(received)],
        },
    }]}))
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"), XDG_CONFIG_HOME=str(directory / "config"),
               XDG_CACHE_HOME=str(directory / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}",
               TAURI_WEBVIEW_AUTOMATION="true", GDK_BACKEND=args.backend, GTK_IM_MODULE=args.input_method, DEVCENTER_DEBUG="1",
               TERMINAL_VALIDATION_RUN=str(directory))
    if args.input_method == "ibus":
        env["IBUS_ADDRESS"] = subprocess.check_output(["ibus", "address"], text=True).strip()
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    client = native.WebDriver(f"http://127.0.0.1:{port}")
    report = {"backend": args.backend, "artifacts": str(directory), "inputMethod": args.input_method, "checks": []}
    driver, tab = None, None

    def mark(description):
        report["checks"].append(description)
        print("PASS:", description, flush=True)

    def focus():
        assert client.js("const input=document.querySelector('#tab-panel-'+arguments[0]+' textarea');input.focus();return document.activeElement===input", tab)

    def expect(before, expected):
        native.wait_for(lambda: received.read_bytes()[before:] == expected,
                        f"exact PTY input {expected.hex()}")

    def events():
        return client.js("return window.keyboardEvents")

    def capture(name):
        (directory / f"{name}.txt").write_text(client.screen(tab))
        (directory / f"{name}.png").write_bytes(base64.b64decode(client.request("GET", "/screenshot")))

    try:
        with (directory / "driver.log").open("w") as log:
            driver = subprocess.Popen([args.driver, "--host=127.0.0.1", f"--port={port}"], env=env,
                                      stdout=log, stderr=log, start_new_session=True)
        native.wait_for(lambda: client.request("GET", "/status", session=False), "WebDriver ready")
        session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "webkitgtk:browserOptions": {"binary": str(Path(args.application).resolve())}
        }}}, session=False)
        client.session = session["sessionId"]
        native.wait_for(lambda: client.find('[aria-label="Create session"]'), "application ready")
        client.invoke("create_session", {"name": "Keyboard acceptance", "directory": str(directory), "projectId": 1})
        client.request("POST", "/refresh", {})
        tab = native.wait_for(lambda: client.js("return Number(document.querySelector('[role=tab][aria-selected=true]')?.id.slice(4)) || null"), "input tab")
        native.wait_for(lambda: "INPUT READY" in client.screen(tab), "raw input fixture ready")
        focus()
        # Literal expected xterm-compatible bytes, independent of the encoder.
        function_keys = [b"\x1bOP", b"\x1bOQ", b"\x1bOR", b"\x1bOS", b"\x1b[15~", b"\x1b[17~",
                         b"\x1b[18~", b"\x1b[19~", b"\x1b[20~", b"\x1b[21~", b"\x1b[23~", b"\x1b[24~"]
        for number, expected in enumerate(function_keys, 1):
            before = received.stat().st_size
            client.keys(chr(0xe030 + number))
            expect(before, expected)
        mark("F1–F12 deliver their exact terminal escape sequences through native WebKit")
        client.js("""window.keyboardEvents=[];
            const input=document.querySelector('#tab-panel-'+arguments[0]+' textarea');
            for(const type of ['compositionstart','compositionupdate','compositionend','beforeinput','input','keydown','keyup'])
              input.addEventListener(type,e=>{if(window.keyboardEvents.length<128)
                window.keyboardEvents.push({type:e.type,data:e.data,trusted:e.isTrusted,isComposing:e.isComposing,key:e.key,code:e.code,ctrl:e.ctrlKey,shift:e.shiftKey});});""", tab)
        focus()
        assert client.invoke("plugin:window|is_focused", {"label": "main"}), "Test window must have native focus"
        with GnomeKeyboard() as keyboard:
            def preedit(codepoint):
                keyboard.keysym(0xffe3, True)  # Control_L
                keyboard.keysym(0xffe1, True)  # Shift_L
                keyboard.tap_sym(ord("U"))
                keyboard.keysym(0xffe1, False)
                keyboard.keysym(0xffe3, False)
                for digit in codepoint:
                    keyboard.tap_sym(ord(digit))
                native.wait_for(lambda: any(event["type"] == "compositionupdate" and event["trusted"] for event in events())
                                or sum(event["type"] == "keyup" and event["trusted"] for event in events()) >= len(codepoint) + 3,
                                "native composition keys processed")
                report.setdefault("preeditObserved", []).append(any(event["type"] == "compositionupdate" for event in events()))

            before = received.stat().st_size
            preedit("65e5")  # 日
            assert received.stat().st_size == before, "Preedit leaked into the PTY"
            capture("preedit")
            keyboard.tap_sym(0xff0d)  # Enter commits composition, not a shell newline.
            expect(before, "日".encode())
            native.wait_for(lambda: any(event["type"] == "compositionend" and event["data"] == "日" and event["trusted"] for event in events()), "one committed composition")
            report["committedCompositionEvents"] = events()
            mark("Native GTK composition keeps preedit out of the PTY and commits 日 exactly once")
            client.js("window.keyboardEvents=[]")
            before = received.stat().st_size
            preedit("3042")  # あ, deliberately cancelled.
            keyboard.tap_sym(0xff1b)  # Escape
            if report["preeditObserved"][-1]:
                native.wait_for(lambda: any(event["type"] == "compositionend" and not event["data"] and event["trusted"] for event in events()), "native composition cancelled")
            else:
                # XWayland may expose composition only on commit. There is no
                # DOM composition to end when GTK cancels its internal preedit.
                native.wait_for(lambda: any(event["type"] == "keyup" and event.get("key") == "Escape" and event["trusted"] for event in events()), "native cancellation key processed")
            assert received.stat().st_size == before, "Cancelled composition leaked into PTY"
            keyboard.tap_sym(ord("x"))
            expect(before, b"x")
            mark("Cancelling native composition sends no preedit text and restores normal typing")
        for text in ["e\u0301", "A\u030a", "n\u0303", "👩‍💻"]:
            before = received.stat().st_size
            client.keys(text)
            expect(before, text.encode())
        assert received.read_bytes() == b"".join(function_keys) + "日xe\u0301A\u030an\u0303👩‍💻".encode(), "Unexpected or duplicated input across the complete sequence"
        capture("graphemes")
        mark("Combining accents and a ZWJ emoji reach the PTY as exact UTF-8")
        report["compositionEvents"] = events()
        capture("composition-finished")
        report["result"] = "passed"
    except BaseException as error:
        report["result"], report["error"] = "failed", str(error)
        if client.session and tab:
            try:
                capture("failure")
                report["compositionEvents"] = events()
                report["receivedHex"] = received.read_bytes().hex()
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
        report["cleanupSurvivors"] = native.owned_processes(directory)
        for process in report["cleanupSurvivors"]:
            try:
                os.kill(process["pid"], signal.SIGKILL)
            except ProcessLookupError:
                pass
        (directory / "report.json").write_text(json.dumps(report, indent=2))
        print("Report:", directory / "report.json", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["wayland", "x11"], default="wayland")
    parser.add_argument("--input-method", choices=["simple", "ibus"], default="simple")
    parser.add_argument("--driver", default="WebKitWebDriver")
    parser.add_argument("--application", default=str(native.ROOT / "src-tauri/target/debug/devcenter-gui"))
    run(parser.parse_args())
