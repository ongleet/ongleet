"""Isolated WebKitGTK regression: one pending terminal must not lock sibling UI.

Uses real Ghostty emulators and PTYs. Only the new tab's frontend connection is held,
then released after closure to exercise a late completion without a process leak.
"""
import argparse, importlib.util, json, os, sqlite3, socket, subprocess, tempfile, time
from pathlib import Path
root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--application', required=True)
parser.add_argument('--driver', required=True)
args = parser.parse_args()
spec = importlib.util.spec_from_file_location('native', root / 'scripts/check-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
directory = Path(tempfile.mkdtemp(prefix='terminal-startup-validation-'))
directory.chmod(0o700)
data = directory / 'data/ongleet'
data.mkdir(parents=True)
c = sqlite3.connect(data / 'workspaces.sqlite3')
for sql in sorted((root / 'src-tauri/migrations').glob('*.sql')):
    c.executescript(sql.read_text())
c.execute('update projects set root_directory=?', (str(directory),))
config = {'version': 1, 'config': {'executable': '/usr/bin/python3', 'args': [str(root / 'scripts/terminal-probe.py'), 'reload']}}
for session in range(1, 7):
    c.execute('insert into sessions values (?,?,?, ?,NULL,1,NULL)', (session, 'Fixture ' + str(session), str(directory), session - 1))
    for index in range(2):
        tab = (session - 1) * 2 + index + 1
        c.execute("insert into tab_definitions values (?,?,?,'terminal',?,?)", (tab, session, 'Probe ' + str(tab), json.dumps(config), index))
        c.execute('insert into open_tabs values (?,?)', (tab, int(tab != 11)))
        history = data / 'terminal-history' / str(session)
        history.mkdir(parents=True, exist_ok=True)
        (history / f'{tab}.txt').write_text(('old history ' + 'x' * 65 + '\n') * 700)
    c.execute('update sessions set selected_tab_id=? where id=?', (session * 2, session))
c.execute('update sessions set selected_tab_id=11 where id=6')
c.execute('update app_state set session_sequence=6,tab_sequence=12,selected_session_id=6')
c.commit()
c.close()
configdir = directory / 'config/ongleet/projects/1'
configdir.mkdir(parents=True)
renamed_source = directory / 'before-rename'
renamed_source.mkdir()
project_actions = [
    {'id': 'probe', 'name': 'New probe', 'kind': 'terminal', 'config': config['config']},
    {'id': 'missing-folder', 'name': 'Missing folder', 'kind': 'terminal', 'config': {**config['config'], 'cwdOverride': str(renamed_source)}},
    {'id': 'missing-executable', 'name': 'Missing executable', 'kind': 'terminal', 'config': {'executable': str(directory / 'not-installed'), 'args': []}},
    {'id': 'nonzero', 'name': 'Nonzero exit', 'kind': 'terminal', 'config': {'executable': '/usr/bin/python3', 'args': ['-c', 'raise SystemExit(23)']}},
]
(configdir / 'actions.yaml').write_text(json.dumps({'actions': project_actions}))
env = dict(os.environ, XDG_DATA_HOME=str(directory / 'data'), XDG_CONFIG_HOME=str(directory / 'config'), XDG_CACHE_HOME=str(directory / 'cache'), GDK_BACKEND='x11', TAURI_WEBVIEW_AUTOMATION='true', TERMINAL_VALIDATION_RUN=str(directory))
with socket.socket() as s:
    s.bind(('127.0.0.1', 0))
    port = s.getsockname()[1]
client = native.WebDriver(f'http://127.0.0.1:{port}')
with (directory / 'driver.log').open('w') as log:
    driver = subprocess.Popen([args.driver, '--host=127.0.0.1', f'--port={port}'], env=env, stdout=log, stderr=log)
    try:
        native.wait_for(lambda: client.request('GET', '/status', session=False), 'driver')
        started = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {'webkitgtk:browserOptions': {'binary': args.application}}}}, session=False)
        client.session = started['sessionId']
        native.wait_for(lambda: client.js('return !!document.querySelector("footer button:not(:disabled)")'), 'restoration', timeout=40)
        native.wait_for(lambda: all((client.state(tab) and client.state(tab).get('sessionId') and (not client.state(tab)['replaying']) for tab in list(range(1, 11)) + [12])), 'all eleven restored PTYs', timeout=40)
        original_runtime = client.state(12)['sessionId']
        client.js('window.__terminalValidation.holdNextConnection()')
        client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", 'xpath')
        client.click("//dialog[@open]//button[normalize-space()='New probe']", 'xpath')
        native.wait_for(lambda: client.js('return window.__terminalValidation.pendingConnection()===13'), 'new terminal waiting for connect')
        assert client.js('return document.querySelector("#tab-panel-13").getAttribute("aria-busy")==="true"')
        assert client.js('return !document.querySelector(".session-choice").disabled'), 'Pending terminal disabled the entire sidebar'
        assert client.js('return !document.getElementById("tab-11").disabled'), 'Pending terminal disabled its sibling tab'
        assert client.js('return document.querySelector("footer button").disabled'), 'Quit must wait for pending attachment'
        client.click('#tab-11')
        native.wait_for(lambda: client.js('return document.querySelector("#tab-11").getAttribute("aria-selected")==="true"'), 'sibling tab selectable')
        client.click("//button[contains(@class,'session-choice')][.//span[normalize-space()='Fixture 1']]", 'xpath')
        native.wait_for(lambda: client.js('return !document.querySelectorAll(".session-pane")[0].hidden'), 'sibling session selectable')
        client.click("//button[contains(@class,'session-choice')][.//span[normalize-space()='Fixture 6']]", 'xpath')
        native.wait_for(lambda: not client.js('return document.querySelectorAll(".session-pane")[5].hidden'), 'return to pending session')
        client.click('#tab-13 + .tab-close')
        native.wait_for(lambda: client.js('return !document.getElementById("tab-13") && !document.querySelector("footer button").disabled'), 'close pending terminal releases restoration guard')
        client.js('window.__terminalValidation.releaseConnection()')
        time.sleep(0.5)
        snapshot = client.invoke('sessions')
        tab = next((t for session in snapshot['sessions'] for t in session['tabs'] if t['id'] == 13))
        assert not tab['isOpen'] and (not tab['resumeOnStart'])
        assert client.invoke('application_status')['runningTerminals'] == 11, 'Late connect launched a closed terminal'
        assert client.state(12)['sessionId'] == original_runtime
        print('PASS: pending startup preserves tab/session navigation, can be closed, releases Quit guard and never launches after closure; eleven sibling PTYs retained', flush=True)
        renamed_source.rename(directory / 'after-rename')
        for action, tab_id, expected in [('Missing folder', 14, 'Working directory'), ('Missing executable', 15, 'No such file or directory')]:
            client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", 'xpath')
            client.click(f"//dialog[@open]//button[normalize-space()='{action}']", 'xpath')
            native.wait_for(lambda: client.js("const panel=document.getElementById('tab-panel-'+arguments[0]); return panel?.getAttribute('aria-busy')==='false' && !!panel.querySelector('[role=alert]')", tab_id), action+' reports launch error')
            assert expected.lower() in client.js("return document.querySelector('#tab-panel-'+arguments[0]+' [role=alert]').innerText", tab_id).lower()
            assert not client.js('return document.querySelector(".session-choice").disabled')
            assert not client.js('return document.querySelector("footer button").disabled')
            client.click('#tab-12')
            native.wait_for(lambda: client.js('return document.getElementById("tab-12").getAttribute("aria-selected")==="true"'), 'live sibling selected after launch failure')
            assert client.state(12)['sessionId'] == original_runtime
            assert client.invoke('application_status')['runningTerminals'] == 11
            snapshot = client.invoke('sessions')
            failed = next(t for session in snapshot['sessions'] for t in session['tabs'] if t['id'] == tab_id)
            assert not failed['resumeOnStart'], 'Failed launch remained restart-eligible'
            client.click(f'#tab-{tab_id} + .tab-close')
            native.wait_for(lambda: client.js('return !document.getElementById("tab-"+arguments[0])', tab_id), 'failed tab closes')
        assert not renamed_source.exists() and (directory / 'after-rename').is_dir()
        client.click("//section[contains(@class,'session-pane') and not(@hidden)]//button[normalize-space()='+ New tab']", 'xpath')
        client.click("//dialog[@open]//button[normalize-space()='Nonzero exit']", 'xpath')
        native.wait_for(lambda: client.js('return document.getElementById("tab-16")?.innerText.includes("(23)")'), 'nonzero process exit displayed')
        assert not client.js('return document.querySelector(".session-choice").disabled')
        assert not client.js('return document.querySelector("footer button").disabled')
        client.click('#tab-12')
        native.wait_for(lambda: client.js('return document.getElementById("tab-12").getAttribute("aria-selected")==="true"'), 'sibling remains selectable after process exit')
        assert client.state(12)['sessionId'] == original_runtime
        assert client.invoke('application_status')['runningTerminals'] == 11
        print('PASS: renamed working directory, missing executable and process exit 23 remain isolated; sibling UI, Quit and eleven PTYs stay available', flush=True)

    except Exception:
        try:
            (directory / 'failure.json').write_text(json.dumps(client.js('return {pending:window.__terminalValidation.pendingConnection(),body:document.body.innerText}'), indent=2))
        except Exception:
            pass
        raise
    finally:
        try:
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
        except Exception:
            pass
        time.sleep(1)
        driver.terminate()
        driver.wait(timeout=5)
        for process in native.owned_processes(directory):
            try:
                os.kill(process['pid'], 9)
            except ProcessLookupError:
                pass
        print('artifacts', directory, flush=True)
