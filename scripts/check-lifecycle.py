"""Native X11/XWayland lifecycle checks with isolated application metadata.

Uses the same bundled native-validation build and WebKitWebDriver as check-native.
XSendEvent delivers WM_DELETE_WINDOW to the test-owned window, exercising Tauri's
actual CloseRequested path instead of replacing it with a test-only backend API.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import tempfile
import time
import importlib.util
spec = importlib.util.spec_from_file_location("native_checks", Path(__file__).with_name("check-native.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
WebDriver, wait_for = native.WebDriver, native.wait_for


def close_window(pid):
    x = ctypes.CDLL("libX11.so.6")
    x.XOpenDisplay.restype = ctypes.c_void_p
    display = x.XOpenDisplay(None)
    if not display:
        raise RuntimeError("X11 display unavailable")
    x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    x.XDefaultRootWindow.restype = ctypes.c_ulong
    x.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
    x.XInternAtom.restype = ctypes.c_ulong
    x.XQueryTree.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_uint)]
    x.XGetWindowProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_long, ctypes.c_long, ctypes.c_int, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ubyte))]
    x.XGetGeometry.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint)]
    x.XFree.argtypes = [ctypes.c_void_p]
    x.XGetWMProtocols.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)), ctypes.POINTER(ctypes.c_int)]
    x.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.c_void_p]
    x.XFlush.argtypes = [ctypes.c_void_p]
    x.XCloseDisplay.argtypes = [ctypes.c_void_p]
    pid_atom = x.XInternAtom(display, b"_NET_WM_PID", 0)
    def find(window):
        actual, count, after = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.c_ulong()
        fmt, data = ctypes.c_int(), ctypes.POINTER(ctypes.c_ubyte)()
        x.XGetWindowProperty(display, window, pid_atom, 0, 1, 0, 6, ctypes.byref(actual), ctypes.byref(fmt), ctypes.byref(count), ctypes.byref(after), ctypes.byref(data))
        matches = bool(data) and count.value == 1 and ctypes.cast(data, ctypes.POINTER(ctypes.c_ulong))[0] == pid
        if data: x.XFree(data)
        protocols, protocol_count = ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_int()
        x.XGetWMProtocols(display, window, ctypes.byref(protocols), ctypes.byref(protocol_count))
        closable = any(protocols[index] == x.XInternAtom(display, b"WM_DELETE_WINDOW", 0) for index in range(protocol_count.value))
        if protocols: x.XFree(protocols)
        geometry_root, gx, gy = ctypes.c_ulong(), ctypes.c_int(), ctypes.c_int()
        width, height, border, depth = ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint(), ctypes.c_uint()
        x.XGetGeometry(display, window, ctypes.byref(geometry_root), ctypes.byref(gx), ctypes.byref(gy), ctypes.byref(width), ctypes.byref(height), ctypes.byref(border), ctypes.byref(depth))
        if matches and closable and width.value > 100: return window
        root, parent, children, n = ctypes.c_ulong(), ctypes.c_ulong(), ctypes.POINTER(ctypes.c_ulong)(), ctypes.c_uint()
        x.XQueryTree(display, window, ctypes.byref(root), ctypes.byref(parent), ctypes.byref(children), ctypes.byref(n))
        result = None
        for index in range(n.value):
            result = find(children[index])
            if result: break
        if children: x.XFree(children)
        return result
    class Message(ctypes.Structure):
        _fields_ = [("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int), ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong), ("format", ctypes.c_int), ("data", ctypes.c_long * 5)]
    try:
        window = find(x.XDefaultRootWindow(display))
        assert window, "No window belonging to test app"
        event = (ctypes.c_long * 24)()
        message = ctypes.cast(event, ctypes.POINTER(Message)).contents
        message.type, message.send_event, message.display, message.window, message.format = 33, 1, display, window, 32
        message.message_type = x.XInternAtom(display, b"WM_PROTOCOLS", 0)
        message.data[0] = x.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
        assert x.XSendEvent(display, window, 0, 0, event)
        x.XFlush(display)
    finally:
        x.XCloseDisplay(display)


def run(args):
    directory = Path(tempfile.mkdtemp(prefix="terminal-lifecycle-validation-"))
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"), XDG_CONFIG_HOME=str(directory / "config"), XDG_CACHE_HOME=str(directory / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}", GDK_BACKEND="x11", TAURI_WEBVIEW_AUTOMATION="true", TERMINAL_VALIDATION_RUN=str(directory))
    config = directory / "config/io.local.terminal-stack-prototype"
    config.mkdir(parents=True)
    (config / "tab-types.yaml").write_text(json.dumps({"tab_types": [{"id": "stream", "name": "Stream", "kind": "terminal", "config": {"executable": "/usr/bin/python3", "args": [str(native.ROOT / "scripts/terminal-probe.py"), "reload"]}}]}))
    data = directory / "data/io.local.terminal-stack-prototype"
    value = 0xcbf29ce484222325
    for byte in os.fsencode(data): value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    path = f"/run/user/{os.getuid()}/terminal-workspace-{value:016x}.sock"
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); port = listener.getsockname()[1]
    report = {"artifacts": str(directory), "checks": []}
    client = WebDriver(f"http://127.0.0.1:{port}")
    def mark(text):
        report["checks"].append(text); print("PASS:", text, flush=True)
    def connect():
        session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {"webkitgtk:browserOptions": {"binary": args.application}}}}, session=False)
        client.session = session["sessionId"]
        wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create workspace\"]:not(:disabled)')"), "frontend ready")
        with socket.socket(socket.AF_UNIX) as peer:
            peer.connect(path)
            pid, _, _ = struct.unpack("3i", peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        return pid
    def pid_alive(pid):
        try: return not Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].lstrip().startswith("Z")
        except FileNotFoundError: return False
    def quit_app(pid, ui=False):
        if ui:
            wait_for(lambda: client.js("return !document.querySelector('footer button').disabled"), "Quit enabled")
            client.click("footer button")
            wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), "Quit warning")
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Confirm']", "xpath")
        else:
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
        wait_for(lambda: not pid_alive(pid), "application Quit")
        try: client.request("DELETE", "")
        except Exception: pass
        client.session = None
    def workgroups_check():
        first_root, second_root = directory / "projects-one", directory / "projects-two"
        first_root.mkdir(); second_root.mkdir()
        (first_root / "keep").write_text("first")
        (second_root / "keep").write_text("second")
        groups = []
        for name, root in [("Group One", first_root), ("Group Two", second_root)]:
            client.click('[aria-label="Create workgroup"]')
            client.fill('dialog[aria-label="Workgroup editor"] label:nth-of-type(1) input', name)
            client.fill('dialog[aria-label="Workgroup editor"] label:nth-of-type(2) input', str(root))
            client.click("//dialog[@open]//button[normalize-space()='Save workgroup']", "xpath")
            group = wait_for(lambda: next((g for g in client.invoke("workspaces")["workgroups"] if g["name"] == name), None), "new workgroup")
            path = Path(client.invoke("workgroup_config", {"id": group["id"]}))
            path.write_text(json.dumps({"tab_types": [{"id": "action", "name": name + " action", "kind": "terminal", "config": {"executable": "/usr/bin/python3", "args": [str(native.ROOT / "scripts/terminal-probe.py"), "reload"]}}]}))
            groups.append(group)
        first, second = groups
        client.click(f'[data-workgroup-id="{first["id"]}"] [aria-label="Create workspace"]')
        wait_for(lambda: client.js("return document.querySelector('#workspace-directory')?.value") == str(first_root), "workgroup default directory")
        client.fill("#workspace-name", "First project")
        client.click("//dialog[@open]//button[normalize-space()='Save workspace']", "xpath")
        workspace = wait_for(lambda: next((w for w in client.invoke("workspaces")["workspaces"] if w["name"] == "First project"), None), "new grouped workspace")
        wait_for(lambda: (state := client.state(client.invoke("workspaces")["workspaces"][-1]["selectedTabId"])) and state.get("cols"), "initial terminal frontend ready")
        client.click("//section[contains(@class,'workspace-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", "xpath")
        wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]')?.innerText.includes('Group One action')"), "scoped actions")
        assert not client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]').innerText.includes('Group Two action')")
        client.click("//dialog[@open]//button[normalize-space()='Group One action']", "xpath")
        tab = wait_for(lambda: client.invoke("workspaces")["workspaces"][0]["selectedTabId"], "selected action")
        wait_for(lambda: "TICK=" in client.screen(tab), "group action running")
        terminal_pid = next(int(line.split("=",1)[1].split()[0]) for line in client.screen(tab).splitlines() if line.startswith("PID="))
        assert Path(f"/proc/{terminal_pid}/cwd").resolve() == first_root
        session_id = client.state(tab)["sessionId"]
        mark("Workgroup creation defaults workspace path and New tab shows only that group's YAML actions")
        before = client.screen(tab)
        client.click(f'[data-workgroup-id="{first["id"]}"] .workgroup-toggle')
        wait_for(lambda: client.js("return document.getElementById('workgroup-'+arguments[0]).hidden", first["id"]), "folded workspace list")
        time.sleep(.4)
        assert client.state(tab)["sessionId"] == session_id and client.screen(tab) != before
        client.request("POST", "/refresh", {})
        wait_for(lambda: client.js("return document.getElementById('workgroup-'+arguments[0])?.hidden", first["id"]), "persisted fold")
        wait_for(lambda: (state := client.state(tab)) and state.get("sessionId") == session_id and not state["replaying"], "same grouped PTY after reload")
        mark("Folding hides workspace entries, preserves active output and persists through reload without relaunch")
        client.click(f'#workgroup-menu-{first["id"]}')
        client.click("//button[@role='menuitem'][normalize-space()='Edit workgroup']", "xpath")
        client.fill('dialog[aria-label="Workgroup editor"] label:nth-of-type(1) input', "Group One renamed")
        client.fill('dialog[aria-label="Workgroup editor"] label:nth-of-type(2) input', str(second_root))
        assert client.js("return Array.from(document.querySelectorAll('dialog[aria-label=\"Workgroup editor\"] input')).some(input=>input.readOnly && input.value.endsWith('/actions.yaml'))")
        client.click("//dialog[@open]//button[normalize-space()='Save workgroup']", "xpath")
        wait_for(lambda: client.invoke("workspaces")["workgroups"][1]["name"] == "Group One renamed", "group edited")
        assert Path(f"/proc/{terminal_pid}/cwd").resolve() == first_root
        assert client.state(tab)["sessionId"] == session_id
        client.click(f'[data-workgroup-id="{second["id"]}"] [aria-label="Create workspace"]')
        wait_for(lambda: client.js("return document.querySelector('#workspace-directory')?.value") == str(second_root), "second group default")
        client.fill("#workspace-name", "Overridden project")
        client.fill("#workspace-directory", str(first_root))
        client.click("//dialog[@open]//button[normalize-space()='Save workspace']", "xpath")
        overridden = wait_for(lambda: next((w for w in client.invoke("workspaces")["workspaces"] if w["name"] == "Overridden project"), None), "override workspace")
        assert overridden["baseDirectory"] == str(first_root) and overridden["workgroupId"] == second["id"]
        wait_for(lambda: (state := client.state(client.invoke("workspaces")["workspaces"][-1]["selectedTabId"])) and state.get("cols"), "initial terminal frontend ready")
        client.click("//section[contains(@class,'workspace-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", "xpath")
        wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]')?.innerText.includes('Group Two action')"), "second group actions")
        assert not client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]').innerText.includes('Group One action')")
        client.click("//dialog[@open]//button[normalize-space()='Cancel']", "xpath")
        mark("Workgroup editing exposes its config path, preserves running cwd, and workspace directory overrides work")
        client.invoke("close_workspace", {"id": workspace["id"], "confirmed": True})
        client.invoke("close_workspace", {"id": overridden["id"], "confirmed": False})
        for group in groups: client.invoke("close_workgroup", {"id": group["id"]})
        assert list(first_root.iterdir()) == [first_root / "keep"]
        assert list(second_root.iterdir()) == [second_root / "keep"]
        client.request("POST", "/refresh", {})
        wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create workspace\"]:not(:disabled)')"), "ready after group checks")

    with (directory / "driver.log").open("w") as log:
        driver = subprocess.Popen([args.driver, "--host=127.0.0.1", f"--port={port}"], env=env, stdout=log, stderr=log)
        try:
            wait_for(lambda: client.request("GET", "/status", session=False), "driver ready")
            pid = connect()
            if args.workgroups: workgroups_check()
            workspace = client.invoke("create_workspace", {"name": "Lifecycle", "directory": str(directory)})["workspaces"][0]
            snapshot = client.invoke("create_tab", {"workspaceId": workspace["id"], "typeId": "stream"})
            tab = snapshot["workspaces"][0]["selectedTabId"]
            client.request("POST", "/refresh", {})
            wait_for(lambda: "TICK=" in client.screen(tab), "stream running")
            before = client.screen(tab)
            stream_pid = next(int(line.split("=",1)[1].split()[0]) for line in before.splitlines() if line.startswith("PID="))
            assert os.stat(path).st_mode & 0o777 == 0o600
            with socket.socket(socket.AF_UNIX) as peer:
                peer.connect(path); peer.sendall(b"x" * 1025); peer.shutdown(socket.SHUT_WR)
                assert b"invalid_request" in peer.recv(1024)
            mark("Private mode-0600 socket rejects oversized requests")
            try: client.invoke("quit", {"confirmed": False})
            except RuntimeError as error: assert "confirmation" in str(error)
            else: raise AssertionError("Unconfirmed Quit accepted")
            mark("Backend rejects Quit while a process runs without explicit confirmation")
            wait_for(lambda: client.js("return !document.querySelector('footer button').disabled"), "Quit enabled")
            client.click("footer button")
            wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), "Quit warning")
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Cancel']", "xpath")
            wait_for(lambda: client.js("return !document.querySelector('footer button').disabled"), "Quit cancellation")
            assert pid_alive(stream_pid) and client.invoke("application_status")["runningSessions"] == 1
            mark("Cancelling the Quit confirmation keeps the live session intact")
            close_window(pid)
            wait_for(lambda: not client.invoke("application_status")["visible"], "window hidden")
            time.sleep(2)
            assert pid_alive(stream_pid)
            subprocess.run([args.application, "--show"], env=env, check=True, timeout=5)
            wait_for(lambda: client.invoke("application_status")["visible"], "window reopened")
            assert client.screen(tab) != before and f"PID={stream_pid}" in client.screen(tab)
            assert client.invoke("application_status")["runningSessions"] == 1
            mark("Native window close hides it; output continues and --show retains the same PTY")
            subprocess.run([args.application], env=env, check=True, timeout=5)
            assert client.invoke("application_status")["runningSessions"] == 1
            mark("Second launch activates the existing instance without duplicate processes")
            quit_app(pid, ui=True)
            wait_for(lambda: not pid_alive(stream_pid), "PTY stopped after Quit")
            assert not Path(path).exists()
            unavailable = subprocess.run([args.application, "--show"], env=env, capture_output=True, timeout=5)
            assert unavailable.returncode != 0 and b"unavailable" in unavailable.stderr
            mark("Explicit Quit terminates PTY, removes socket; --show fails without starting an app")
            new_pid = connect()
            wait_for(lambda: "TICK=" in client.screen(tab), "formerly running command restarted")
            new_stream = next(int(line.split("=",1)[1].split()[0]) for line in client.screen(tab).splitlines() if line.startswith("PID="))
            assert new_stream != stream_pid
            assert client.invoke("workspaces")["workspaces"][0]["id"] == workspace["id"]
            quit_app(new_pid)
            wait_for(lambda: not pid_alive(new_stream), "restored PTY stopped")
            mark("Restart restores metadata and launches a fresh process under the selected resume policy")
            report["result"] = "passed"
        except BaseException as error:
            report["result"], report["error"] = "failed", str(error)
            if client.session:
                try: (directory / "failure-screen.txt").write_text(client.js("return document.body.innerText"))
                except Exception: pass
            raise
        finally:
            driver.terminate()
            try: driver.wait(timeout=5)
            except subprocess.TimeoutExpired: driver.kill(); driver.wait()
            survivors = native.owned_processes(directory)
            report["cleanupSurvivors"] = survivors
            for process in survivors:
                try: os.kill(process["pid"], signal.SIGKILL)
                except ProcessLookupError: pass
            (directory / "report.json").write_text(json.dumps(report, indent=2))
            print("Report:", directory / "report.json", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workgroups", action="store_true", help="Also check grouped workspace UI, per-group YAML and fold persistence")
    parser.add_argument("--driver", required=True)
    parser.add_argument("--application", default=str(native.ROOT / "src-tauri/target/debug/terminal-workspace-prototype"))
    run(parser.parse_args())
