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
import shutil
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
    if args.replace_executable:
        # Mutate only this harness-owned copy, never a user/build executable.
        copy = directory / "application"
        shutil.copy2(args.application, copy)
        args.application = str(copy)
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"), XDG_CONFIG_HOME=str(directory / "config"), XDG_CACHE_HOME=str(directory / "cache"), XDG_RUNTIME_DIR=f"/run/user/{os.getuid()}", GDK_BACKEND="x11", TAURI_WEBVIEW_AUTOMATION="true", TERMINAL_VALIDATION_RUN=str(directory))
    plugin_checks = None
    if args.plugins:
        plugin_spec = importlib.util.spec_from_file_location("plugin_checks", Path(__file__).with_name("plugin-native-checks.py"))
        plugin_checks = importlib.util.module_from_spec(plugin_spec)
        plugin_spec.loader.exec_module(plugin_checks)
        plugin_checks.prepare(directory, env)
    config = directory / "config/devcenter"
    config.mkdir(parents=True)
    (config / "projects/1").mkdir(parents=True)
    (config / "projects/1/actions.yaml").write_text(json.dumps({"actions": [{"id": "stream", "name": "Stream", "kind": "terminal", "config": {"executable": "/usr/bin/python3", "args": [str(native.ROOT / "scripts/terminal-probe.py"), "reload"]}}]}))
    data = directory / "data/devcenter"
    value = 0xcbf29ce484222325
    for byte in os.fsencode(data): value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    path = f"/run/user/{os.getuid()}/devcenter-{value:016x}.sock"
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0)); port = listener.getsockname()[1]
    report = {"artifacts": str(directory), "checks": []}
    client = WebDriver(f"http://127.0.0.1:{port}")
    def mark(text):
        report["checks"].append(text); print("PASS:", text, flush=True)
    def connect():
        session = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {"webkitgtk:browserOptions": {"binary": args.application}}}}, session=False)
        client.session = session["sessionId"]
        wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), "frontend ready")
        with socket.socket(socket.AF_UNIX) as peer:
            peer.connect(path)
            pid, _, _ = struct.unpack("3i", peer.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        return pid
    def pid_alive(pid):
        try: return not Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].lstrip().startswith("Z")
        except FileNotFoundError: return False
    def quit_app(pid, ui=False):
        if ui:
            wait_for(lambda: client.js("return !document.querySelector('footer > button:last-of-type').disabled"), "Quit enabled")
            client.click("footer > button:last-of-type")
            wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), "Quit warning")
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Confirm']", "xpath")
        else:
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
        wait_for(lambda: not pid_alive(pid), "application Quit")
        try: client.request("DELETE", "")
        except Exception: pass
        client.session = None
    def actions_editor_check(group, terminal_pid, session_id, old_tab):
        def settings(view):
            client.click(f'#settings-{view}')
            wait_for(lambda: client.js("const button=document.getElementById('settings-'+arguments[0]);return button?.getAttribute('aria-selected')==='true' && !button.disabled", view), 'settings view ready')
        client.click(f'#project-menu-{group["id"]}')
        client.click("//button[@role='menuitem'][normalize-space()='Edit project']", "xpath")
        settings('actions')
        for name, value in [('EDITOR_COMMON', 'shared'), ('EDITOR_TEST', 'group default')]:
            client.click("//button[normalize-space()='Add variable']", 'xpath')
            count = client.js("return document.querySelectorAll('.env-row').length")
            client.fill(f'.env-row:nth-of-type({count}) [aria-label="Common variable name"]', name)
            client.fill(f'.env-row:nth-of-type({count}) [aria-label="Common variable value"]', value)
        field = '#settings-panel-actions fieldset:first-of-type '

        client.fill(field + '[aria-label="Action name"]', 'Edited action')
        client.fill(field + '[aria-label="Executable"]', '/usr/bin/python3')
        client.fill(field + '[aria-label="Arguments"]', json.dumps(['-u', '-c', "import os,time; print('EDITOR_READY='+os.environ['EDITOR_TEST']+';COMMON='+os.environ['EDITOR_COMMON'],flush=True); time.sleep(60)"]))
        client.fill(field + '[aria-label="Environment overrides"]', '{"EDITOR_TEST":"saved value"}')
        client.click("//button[normalize-space()='Add action']", "xpath")
        client.fill('#settings-panel-actions fieldset:nth-of-type(2) [aria-label="Action name"]', 'Temporary')
        client.click("//fieldset[2]//button[normalize-space()='Move earlier']", "xpath")
        assert client.js("return document.querySelector('#settings-panel-actions fieldset input').value") == 'Temporary'
        client.click("//fieldset[1]//button[normalize-space()='Move later']", "xpath")
        client.click("//fieldset[2]//button[normalize-space()='Remove action']", "xpath")
        settings('yaml')
        yaml = client.js("return document.querySelector('#project-actions-yaml').value")
        assert 'actions:' in yaml and 'EDITOR_COMMON: shared' in yaml and 'Edited action' in yaml and 'tab_types' not in yaml
        config = Path(client.invoke('project_config', {'id': group['id']}))
        original = config.read_text()
        client.fill('#project-actions-yaml', 'actions: []')
        client.click("//button[normalize-space()='Save actions']", "xpath")
        wait_for(lambda: client.js("return Array.from(document.querySelectorAll('[role=alert]')).some(element=>element.innerText.includes('between 1 and 100 actions'))"), 'invalid actions rejected')
        assert config.read_text() == original
        client.fill('#project-actions-yaml', yaml.replace('Edited action', 'Edited from YAML'))
        settings('actions')
        assert client.js("return document.querySelector('#settings-panel-actions fieldset input').value") == 'Edited from YAML'
        client.click("//button[normalize-space()='Save actions']", "xpath")
        wait_for(lambda: client.js("return Array.from(document.querySelectorAll('[role=status]')).some(element=>element.innerText.includes('Actions saved'))"), 'actions saved')
        assert 'Edited from YAML' in config.read_text()
        client.click("//dialog[@open]//button[normalize-space()='Close']", "xpath")
        assert pid_alive(terminal_pid) and client.state(old_tab)['sessionId'] == session_id
        assert b'EDITOR_COMMON=' not in Path(f'/proc/{terminal_pid}/environ').read_bytes()
        client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", 'xpath')
        client.click("//dialog[@open]//button[normalize-space()='Edited from YAML']", 'xpath')
        tab = wait_for(lambda: next((w['selectedTabId'] for w in client.invoke('sessions')['sessions'] if w['projectId'] == group['id'] and w['selectedTabId'] != old_tab), None), 'edited action selected')
        wait_for(lambda: 'EDITOR_READY=saved value;COMMON=shared' in client.screen(tab), 'edited action executes')
        assert pid_alive(terminal_pid) and client.state(old_tab)['sessionId'] == session_id
        client.click(f'#project-menu-{group["id"]}')
        client.click("//button[@role='menuitem'][normalize-space()='Edit project']", 'xpath')
        settings('actions')
        assert client.js("return document.querySelector('#settings-panel-actions fieldset input').value") == 'Edited from YAML'
        assert client.js("return Array.from(document.querySelectorAll('.env-row input')).some(input=>input.value==='EDITOR_COMMON')")
        client.click("//button[normalize-space()='Add action']", 'xpath')
        field = '#settings-panel-actions fieldset:nth-of-type(2) '
        client.fill(field + '[aria-label="Action name"]', 'External probe')
        client.js("const select=document.querySelector(arguments[0]);select.value='command';select.dispatchEvent(new Event('change',{bubbles:true}))", field + '[aria-label="Action type"]')
        client.fill(field + '[aria-label="Executable"]', '/usr/bin/python3')
        artifact = directory / 'external-result.json'
        script = "import os,sys,json; open(" + repr(str(artifact)) + ",'w').write(json.dumps({'args':sys.argv[1:],'cwd':os.getcwd(),'env':os.getenv('SESSION_PATH'),'common':os.getenv('EDITOR_COMMON')}))"
        client.fill(field + '[aria-label="Arguments"]', json.dumps(['-c',script,'{{session.path}}','literal ; $(touch nope)']))
        client.click("//button[normalize-space()='Save actions']", 'xpath')
        wait_for(lambda: 'kind: command' in config.read_text(), 'external action saved')
        settings('yaml')
        assert 'kind: command' in client.js("return document.querySelector('#project-actions-yaml').value")
        settings('actions')
        assert client.js("return document.querySelector(arguments[0]).value", field + '[aria-label="Action type"]') == 'command'
        client.click("//dialog[@open]//button[normalize-space()='Close']", 'xpath')
        session_before = next(w for w in client.invoke('sessions')['sessions'] if w['projectId'] == group['id'])
        client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='Actions']", 'xpath')
        client.click("//dialog[@aria-label='External actions']//button[normalize-space()='External probe']", 'xpath')
        wait_for(artifact.exists, 'external command completed')
        result = json.loads(artifact.read_text())
        assert result['args'] == [session_before['baseDirectory'],'literal ; $(touch nope)']
        assert result['cwd'] == session_before['baseDirectory'] and result['env'] == result['cwd'] and result['common'] == 'shared'
        assert len(next(w for w in client.invoke('sessions')['sessions'] if w['id'] == session_before['id'])['tabs']) == len(session_before['tabs'])
        assert client.state(old_tab)['sessionId'] == session_id and pid_alive(terminal_pid)
        mark('External action form/YAML persists, launches with literal session placeholders and common environment, and creates no tab')
        mark('Common environment form/YAML roundtrip, override precedence and edited command launch preserve the live sibling')

    def projects_check():
        first_root, second_root = directory / "projects-one", directory / "projects-two"
        first_root.mkdir(); second_root.mkdir()
        (first_root / "keep").write_text("first")
        (second_root / "keep").write_text("second")
        groups = []
        for name, root in [("Group One", first_root), ("Group Two", second_root)]:
            client.click('[aria-label="Create project"]')
            client.fill('dialog[aria-label="Project editor"] label:nth-of-type(1) input', name)
            client.fill('dialog[aria-label="Project editor"] label:nth-of-type(2) input', str(root))
            client.click("//dialog[@open]//button[normalize-space()='Save project']", "xpath")
            group = wait_for(lambda: next((g for g in client.invoke("sessions")["projects"] if g["name"] == name), None), "new project")
            path = Path(client.invoke("project_config", {"id": group["id"]}))
            path.write_text(json.dumps({"actions": [{"id": "action", "name": name + " action", "kind": "terminal", "config": {"executable": "/usr/bin/python3", "args": [str(native.ROOT / "scripts/terminal-probe.py"), "reload"]}}]}))
            groups.append(group)
        first, second = groups
        client.click(f'[data-project-id="{first["id"]}"] [aria-label="Create session"]')
        wait_for(lambda: client.js("return document.querySelector('#session-directory')?.value") == str(first_root), "project default directory")
        client.fill("#session-name", "First project")
        client.click("//dialog[@open]//button[normalize-space()='Save session']", "xpath")
        session = wait_for(lambda: next((w for w in client.invoke("sessions")["sessions"] if w["name"] == "First project"), None), "new grouped session")
        wait_for(lambda: (state := client.state(client.invoke("sessions")["sessions"][-1]["selectedTabId"])) and state.get("cols"), "initial terminal frontend ready")
        client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", "xpath")
        wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]')?.innerText.includes('Group One action')"), "scoped actions")
        assert not client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]').innerText.includes('Group Two action')")
        client.click("//dialog[@open]//button[normalize-space()='Cancel']", "xpath")
        assert len(session["tabs"]) == 1 and session["tabs"][0]["name"] == "Group One action"
        tab = session["selectedTabId"]
        wait_for(lambda: "TICK=" in client.screen(tab), "group action running")
        terminal_pid = next(int(line.split("=",1)[1].split()[0]) for line in client.screen(tab).splitlines() if line.startswith("PID="))
        assert Path(f"/proc/{terminal_pid}/cwd").resolve() == first_root
        session_id = client.state(tab)["sessionId"]
        mark("New session automatically starts its first YAML action as its only tab, in the default group path")
        before = client.screen(tab)
        client.click(f'[data-project-id="{first["id"]}"] .project-toggle')
        wait_for(lambda: client.js("return document.getElementById('project-'+arguments[0]).hidden", first["id"]), "folded session list")
        time.sleep(.4)
        assert client.state(tab)["sessionId"] == session_id and client.screen(tab) != before
        client.request("POST", "/refresh", {})
        wait_for(lambda: client.js("return document.getElementById('project-'+arguments[0])?.hidden", first["id"]), "persisted fold")
        wait_for(lambda: (state := client.state(tab)) and state.get("sessionId") == session_id and not state["replaying"], "same grouped PTY after reload")
        mark("Folding hides session entries, preserves active output and persists through reload without relaunch")
        client.click(f'#project-menu-{first["id"]}')
        client.click("//button[@role='menuitem'][normalize-space()='Edit project']", "xpath")
        client.fill('dialog[aria-label="Project editor"] label:nth-of-type(1) input', "Group One renamed")
        client.fill('dialog[aria-label="Project editor"] label:nth-of-type(2) input', str(second_root))
        assert client.js("return Array.from(document.querySelectorAll('dialog[aria-label=\"Project editor\"] input')).some(input=>input.readOnly && input.value.endsWith('/actions.yaml'))")
        client.click("//dialog[@open]//button[normalize-space()='Save project']", "xpath")
        wait_for(lambda: client.invoke("sessions")["projects"][1]["name"] == "Group One renamed", "group edited")
        assert Path(f"/proc/{terminal_pid}/cwd").resolve() == first_root
        assert client.state(tab)["sessionId"] == session_id
        if args.actions_editor: actions_editor_check(first, terminal_pid, session_id, tab)
        client.click(f'[data-project-id="{second["id"]}"] [aria-label="Create session"]')
        wait_for(lambda: client.js("return document.querySelector('#session-directory')?.value") == str(second_root), "second group default")
        client.fill("#session-name", "Overridden project")
        client.fill("#session-directory", str(first_root))
        client.click("//dialog[@open]//button[normalize-space()='Save session']", "xpath")
        overridden = wait_for(lambda: next((w for w in client.invoke("sessions")["sessions"] if w["name"] == "Overridden project"), None), "override session")
        assert overridden["baseDirectory"] == str(first_root) and overridden["projectId"] == second["id"]
        wait_for(lambda: (state := client.state(client.invoke("sessions")["sessions"][-1]["selectedTabId"])) and state.get("cols"), "initial terminal frontend ready")
        client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", "xpath")
        wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]')?.innerText.includes('Group Two action')"), "second group actions")
        assert not client.js("return document.querySelector('dialog[aria-label=\"New tab\"][open]').innerText.includes('Group One action')")
        client.click("//dialog[@open]//button[normalize-space()='Cancel']", "xpath")
        mark("Project editing exposes its config path, preserves running cwd, and session directory overrides work")
        client.invoke("close_session", {"id": session["id"], "confirmed": True})
        client.invoke("close_session", {"id": overridden["id"], "confirmed": True})
        for group in groups: client.invoke("close_project", {"id": group["id"]})
        assert list(first_root.iterdir()) == [first_root / "keep"]
        assert list(second_root.iterdir()) == [second_root / "keep"]
        client.request("POST", "/refresh", {})
        wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), "ready after group checks")

    def cli(*arguments, code=0):
        result = subprocess.run([args.cli, "--json", *map(str, arguments)], env=env, capture_output=True, text=True, timeout=40)
        assert result.returncode == code, (arguments, result.returncode, result.stdout, result.stderr)
        response = json.loads(result.stdout if code == 0 else result.stderr)
        assert response['ok'] == (code == 0), response
        return response.get('data') if code == 0 else response['error']

    def cli_check():
        path_for_cli = path
        from concurrent.futures import ThreadPoolExecutor
        root, other = directory / 'cli-root', directory / 'cli-other'
        root.mkdir(); other.mkdir()
        for entry in [root, other]: (entry / 'keep').write_text('unchanged')
        group = cli('project', 'create', '--name', 'CLI group', '--root', root)
        gid = group['id']
        assert any(g['id'] == gid for g in cli('project', 'list')['projects'])
        cli('project', 'rename', gid, '--name', 'CLI renamed')
        wait_for(lambda: client.js("return document.querySelector('[data-project-id=\"'+arguments[0]+'\"]')?.innerText.includes('CLI renamed')", gid), 'CLI group in sidebar')
        cli('project', 'directory', gid, '--directory', other)
        cli('project', 'directory', gid, '--directory', root)
        # Use a known long-running first action, never the user's configuration.
        config_path = config / f'projects/{gid}'
        config_path.mkdir(parents=True, exist_ok=True)
        actions = json.loads((config / 'projects/1/actions.yaml').read_text())
        probe = str(native.ROOT / 'scripts/terminal-probe.py')
        actions['actions'][0]['config']['args'] = ['-u', '-c',
            "import subprocess,os,json,sys,runpy; result=subprocess.run(['devcenter','--json','session','list'],capture_output=True,text=True); open(" + repr(str(directory / 'cli-on-path.json')) + ", 'w').write(json.dumps({'returncode':result.returncode,'stdout':result.stdout,'stderr':result.stderr})); assert result.returncode==0,result.stderr; print('CLI_ON_PATH',flush=True); sys.argv=[" + repr(probe) + ",'reload']; runpy.run_path(" + repr(probe) + ",run_name='__main__')"]
        (config_path / 'actions.yaml').write_text(json.dumps(actions))
        first = cli('session', 'create', '--project', gid, '--name', 'CLI first')
        wid, tab = first['id'], first['selectedTabId']
        wait_for(lambda: 'TICK=' in client.screen(tab), 'CLI-created action running without reload')
        assert 'CLI_ON_PATH' in client.screen(tab)
        terminal_pid = next(int(line.split('=',1)[1].split()[0]) for line in client.screen(tab).splitlines() if line.startswith('PID='))
        session = client.state(tab)['sessionId']
        assert Path(f'/proc/{terminal_pid}/cwd').resolve() == root
        cli('session', 'rename', wid, '--name', 'CLI renamed session')
        wait_for(lambda: client.js("return document.querySelector('.sidebar')?.innerText.includes('CLI renamed session')"), 'CLI rename displayed')
        cli('session', 'directory', wid, '--directory', other)
        assert Path(f'/proc/{terminal_pid}/cwd').resolve() == root
        cli('project', 'fold', gid)
        wait_for(lambda: client.js("return document.getElementById('project-'+arguments[0])?.hidden", gid), 'CLI fold displayed')
        cli('project', 'unfold', gid)
        with ThreadPoolExecutor(max_workers=2) as pool:
            created = list(pool.map(lambda name: cli('session','create','--project',gid,'--name',name), ['CLI second','CLI third']))
        for item in created: wait_for(lambda: 'TICK=' in client.screen(item['selectedTabId']), 'concurrent session action running')
        cli('session', 'reorder', created[1]['id'], '--position', 0)
        ordered = [w for w in cli('session', 'list')['sessions'] if w['projectId'] == gid]
        assert ordered[0]['id'] == created[1]['id']
        cli('session', 'select', wid)
        wait_for(lambda: client.js("return document.querySelector('.session-pane:not([hidden])')?.getAttribute('aria-label')") == 'CLI renamed session', 'CLI selection displayed')
        assert client.state(tab)['sessionId'] == session and pid_alive(terminal_pid)
        cli('session', 'move', wid, '--project', 1)
        assert client.state(tab)['sessionId'] == session
        assert cli('session','close',wid,code=5)['code'] == 'confirmation_required'
        assert pid_alive(terminal_pid)
        assert cli('session','select',999999,code=4)['code'] == 'not_found'
        cli('session','reorder',wid,'--position',99999,code=2)
        cli('session','create','--project',gid,'--name','Missing','--directory',root/'missing',code=6)
        assert not (root/'missing').exists()
        cli('session','close',wid,'--confirm')
        wait_for(lambda: not pid_alive(terminal_pid), 'CLI close kills owned process')
        for item in created: cli('session','close',item['id'],'--confirm')
        cli('project','close',gid)
        wait_for(lambda: not client.js("return !!document.querySelector('[data-project-id=\"'+arguments[0]+'\"]')",gid), 'CLI deletion displayed')
        assert all(list(path.iterdir()) == [path/'keep'] and (path/'keep').read_text() == 'unchanged' for path in [root,other])
        with ThreadPoolExecutor(max_workers=4) as pool:
            assert all(pool.map(lambda _: cli('app','status')['pid'] > 0, range(8)))
        for payload, error in [(b'x' * (128 * 1024 + 1),'message_too_large'),(b'{"version":99,"command":"status"}','protocol_error'),(b'{"version":2,"command":"session_list","extra":true}','invalid_request')]:
            with socket.socket(socket.AF_UNIX) as peer:
                peer.connect(path_for_cli)
                peer.sendall(payload); peer.shutdown(socket.SHUT_WR)
                assert json.loads(peer.recv(4096))['error']['code'] == error
        mark('CLI session/project CRUD, concurrent creation, live UI, stable PTYs, structured errors and confirmed cleanup preserve directories')

    with (directory / "driver.log").open("w") as log:
        driver = subprocess.Popen([args.driver, "--host=127.0.0.1", f"--port={port}"], env=env, stdout=log, stderr=log)
        try:
            wait_for(lambda: client.request("GET", "/status", session=False), "driver ready")
            pid = connect()
            if args.projects: projects_check()
            if args.cli: cli_check()
            if args.replace_executable:
                application = Path(args.application)
                retired = directory / "retired-application"
                application.rename(retired)
                shutil.copy2(retired, application)
                retired.unlink()
                assert os.readlink(f"/proc/{pid}/exe").endswith(" (deleted)")
            session = client.invoke("create_session", {"name": "Lifecycle", "directory": str(directory)})["sessions"][0]
            assert len(session["tabs"]) == 1 and session["tabs"][0]["name"] == "Stream"
            tab = session["selectedTabId"]
            client.request("POST", "/refresh", {})
            wait_for(lambda: "TICK=" in client.screen(tab), "stream running")
            if args.replace_executable:
                mark("New PTY launches after replacing/unlinking the running application executable")
            before = client.screen(tab)
            stream_pid = next(int(line.split("=",1)[1].split()[0]) for line in before.splitlines() if line.startswith("PID="))
            if plugin_checks:
                saved_plugins = plugin_checks.run(client, directory, session, tab, stream_pid, wait_for, pid_alive, mark, cli if args.cli else None)
            assert os.stat(path).st_mode & 0o777 == 0o600
            with socket.socket(socket.AF_UNIX) as peer:
                peer.connect(path); peer.sendall(b"x" * 1025); peer.shutdown(socket.SHUT_WR)
                assert b"invalid_request" in peer.recv(1024)
            mark("Private mode-0600 socket rejects malformed requests")
            try: client.invoke("quit", {"confirmed": False})
            except RuntimeError as error: assert "confirmation" in str(error)
            else: raise AssertionError("Unconfirmed Quit accepted")
            mark("Backend rejects Quit while a process runs without explicit confirmation")
            wait_for(lambda: client.js("return !document.querySelector('footer > button:last-of-type').disabled"), "Quit enabled")
            client.click("footer > button:last-of-type")
            wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), "Quit warning")
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Cancel']", "xpath")
            wait_for(lambda: client.js("return !document.querySelector('footer > button:last-of-type').disabled"), "Quit cancellation")
            assert pid_alive(stream_pid) and client.invoke("application_status")["runningTerminals"] == 1
            mark("Cancelling the Quit confirmation keeps the live session intact")
            close_window(pid)
            wait_for(lambda: not client.invoke("application_status")["visible"], "window hidden")
            time.sleep(2)
            if args.cli:
                assert cli('app','status')['visible'] is False
                cli('session','rename',session['id'],'--name','Hidden CLI session')
                wait_for(lambda: client.js("return document.querySelector('.sidebar').innerText.includes('Hidden CLI session')"), 'hidden frontend consumes CLI changes')
                assert cli('app','status')['runningTerminals'] == 1
                cli('app','show')
            assert pid_alive(stream_pid)
            subprocess.run([args.application, "--show"], env=env, check=True, timeout=5)
            wait_for(lambda: client.invoke("application_status")["visible"], "window reopened")
            assert client.screen(tab) != before and f"PID={stream_pid}" in client.screen(tab)
            assert client.invoke("application_status")["runningTerminals"] == 1
            mark("Native window close hides it; output continues and --show retains the same PTY")
            subprocess.run([args.application], env=env, check=True, timeout=5)
            assert client.invoke("application_status")["runningTerminals"] == 1
            mark("Second launch activates the existing instance without duplicate processes")
            if plugin_checks: pending_gh = plugin_checks.before_quit(client, directory, session['id'], wait_for)
            quit_app(pid, ui=True)
            if plugin_checks: plugin_checks.after_quit(directory, pending_gh, pid_alive, mark)
            wait_for(lambda: not pid_alive(stream_pid), "PTY stopped after Quit")
            assert not Path(path).exists()
            unavailable = subprocess.run([args.application, "--show"], env=env, capture_output=True, timeout=5)
            assert unavailable.returncode != 0 and b"unavailable" in unavailable.stderr
            mark("Explicit Quit terminates PTY, removes socket; --show fails without starting an app")
            if args.cli:
                assert cli('app','status',code=3)['code'] == 'unavailable'
                assert not Path(path).exists()
            history_file = data / "terminal-history" / str(session["id"]) / f"{tab}.txt"
            assert f"PID={stream_pid}" in history_file.read_text()
            assert history_file.stat().st_mode & 0o777 == 0o600
            new_pid = connect()
            wait_for(lambda: len([line for line in client.screen(tab).splitlines() if line.startswith("PID=")]) >= 2, "saved history and fresh command output")
            if plugin_checks: plugin_checks.restored(client, session['id'], saved_plugins, mark)
            restored_screen = client.screen(tab)
            assert f"PID={stream_pid}" in restored_screen
            assert "Previous terminal history" in restored_screen
            new_stream = [int(line.split("=",1)[1].split()[0]) for line in restored_screen.splitlines() if line.startswith("PID=")][-1]
            runtime_id = client.state(tab)["sessionId"]
            client.request("POST", "/refresh", {})
            wait_for(lambda: client.state(tab) and client.state(tab).get("sessionId") and not client.state(tab)["replaying"], "history reload reattachment")
            assert client.state(tab)["sessionId"] == runtime_id
            assert client.screen(tab).count("Previous terminal history") == 1
            mark("Saved text survives Quit/restart with a distinct fresh process; reload preserves history without duplicating it")
            assert new_stream != stream_pid
            assert client.invoke("sessions")["sessions"][0]["id"] == session["id"]
            if args.cli:
                cli('app','quit',code=5)
                cli('app','quit','--confirm')
                wait_for(lambda: not pid_alive(new_pid), 'CLI Quit')
                client.session = None
            else:
                quit_app(new_pid)
            wait_for(lambda: not pid_alive(new_stream), "restored PTY stopped")
            mark("Restart restores metadata and launches a fresh process under the selected resume policy")
            if args.cli:
                status = cli('app','start','--executable',args.application)
                started_pid = status['pid']
                assert cli('app','start','--executable',args.application)['pid'] == started_pid
                cli('app','show')
                cli('app','quit','--confirm')
                wait_for(lambda: not pid_alive(started_pid), 'explicit CLI-started app Quit')
                mark('CLI unavailable never auto-starts; explicit start, single instance, show, status and confirmed Quit work')
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
    parser.add_argument("--plugins", action="store_true", help="Exercise mixed plugin tabs with file browsing/editing and an isolated gh stand-in")
    parser.add_argument("--cli", help="Path to CLI executable for native shared-state integration checks")
    parser.add_argument("--actions-editor", action="store_true", help="Exercise the project Actions form and YAML editor (with --projects)")
    parser.add_argument("--replace-executable", action="store_true", help="Replace a private copy of the running executable before launching a PTY")
    parser.add_argument("--projects", action="store_true", help="Also check grouped session UI, per-group YAML and fold persistence")
    parser.add_argument("--driver", required=True)
    parser.add_argument("--application", default=str(native.ROOT / "src-tauri/target/debug/devcenter-gui"))
    run(parser.parse_args())
