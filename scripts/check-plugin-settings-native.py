"""Check global/session/tab plugin settings on a private Wayland display and bus."""
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


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


native = module('native', 'check-native.py')
plugins = module('plugins', 'plugin-native-checks.py')


def run(application):
    plugins.require_private_secret_service()
    directory = Path(tempfile.mkdtemp(prefix='ongleet-settings-native-'))
    runtime = directory / 'runtime'
    runtime.mkdir(mode=0o700)
    folder = directory / 'folder'
    folder.mkdir()
    (folder / '.hidden').write_text('fixture')
    (folder / 'visible.txt').write_text('fixture')
    env = dict(os.environ, XDG_DATA_HOME=str(directory/'data'), XDG_CONFIG_HOME=str(directory/'config'),
               XDG_CACHE_HOME=str(directory/'cache'), XDG_RUNTIME_DIR=str(runtime),
               TAURI_WEBVIEW_AUTOMATION='true', TERMINAL_VALIDATION_RUN=str(directory),
               GDK_BACKEND='wayland', WAYLAND_DISPLAY='settings-wayland', GH_TOKEN='first-fixture')
    env.pop('DISPLAY', None)
    env.pop('GITHUB_TOKEN', None)
    checks = []
    report = {'artifacts': str(directory), 'backend': 'private Wayland', 'github': 'fixture HTTP; real private Secret Service', 'checks': checks}
    driver = compositor = keyring = client = None
    def mark(message):
        checks.append(message)
        print(message, flush=True)
    with (directory/'native.log').open('w') as log:
        try:
            keyring = subprocess.Popen(['gnome-keyring-daemon', '--foreground', '--unlock', '--components=secrets'],
                                       stdin=subprocess.PIPE, stdout=log, stderr=log, env=env)
            keyring.stdin.write(b'fixture-password\n')
            keyring.stdin.close()
            compositor = subprocess.Popen(['weston', '--backend=headless', '--renderer=pixman', '--width=1280', '--height=900',
                                           '--socket=settings-wayland', '--idle-time=0', '--no-config', f'--log={directory}/weston.log'],
                                          env=env, stdout=log, stderr=log)
            native.wait_for(lambda: (runtime/'settings-wayland').exists(), 'private display')
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', 0))
                port = listener.getsockname()[1]
            driver = subprocess.Popen(['WebKitWebDriver', '--host=127.0.0.1', f'--port={port}'], env=env, stdout=log, stderr=log)
            client = native.WebDriver(f'http://127.0.0.1:{port}')
            native.wait_for(lambda: client.request('GET', '/status', session=False), 'driver')
            actions = {'actions': [
                {'id':'shell','name':'Shell','kind':'terminal','config':{'executable':'/bin/cat'}},
                {'id':'files','name':'Files','kind':'files','config':{}},
                {'id':'pr','name':'Pull request','kind':'pull-request','config':{}},
            ]}
            def start():
                result = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {'webkitgtk:browserOptions': {'binary': str(Path(application).resolve())}}}}, session=False)
                client.session = result['sessionId']
                native.wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), 'app')
                client.js('window.__prFixture?.enable()')
            def reload():
                client.request('POST', '/refresh', {})
                native.wait_for(lambda: client.js('return !!window.__sdkValidation'), 'reloaded bridge')
                client.js('window.__prFixture.enable()')
            def sdk(tab, service, method, *args):
                native.wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])', tab), 'attachment')
                return plugins.sdk(client, tab, service, method, *args)
            def replace(tab, scope, value):
                previous = sdk(tab, 'storage', 'read', scope)
                return sdk(tab, 'storage', 'replace', scope, previous['revision'] if previous else 0, 1, value)
            def new_tab(session, action):
                state = client.invoke('create_tab', {'sessionId':session, 'actionId': action})
                tab = next(s for s in state['sessions'] if s['id']==session)['selectedTabId']
                reload()
                return tab
            start()
            sessions = []
            for index in [1, 2]:
                state = client.invoke('create_project', {'name':f'Project {index}', 'rootDirectory':str(folder)})
                project = state['projects'][-1]['id']
                client.invoke('save_project_actions', {'id':project, 'yaml':json.dumps(actions)})
                state = client.invoke('create_session', {'name':f'Session {index}', 'directory':str(folder), 'projectId':project})
                sessions.append(state['selectedId'])
            reload()
            a = new_tab(sessions[0], 'files')
            b = new_tab(sessions[0], 'files')
            c = new_tab(sessions[1], 'files')
            def activate(session, tab):
                client.invoke('select_session', {'id':session})
                client.invoke('select_tab', {'id':tab})
                reload()
            def has_hidden(tab):
                return client.js('return !!Array.from(document.querySelectorAll(arguments[0]+" .file-list button")).find(el=>el.textContent.includes(".hidden"))', f'#tab-panel-{tab}')
            activate(sessions[0], a)
            replace(a, 'globalSettings', {'showHidden':True, 'order':'nameDescending'})
            native.wait_for(lambda: has_hidden(a), 'global hidden files')
            replace(a, 'settings', {'showHidden':False})
            native.wait_for(lambda: not has_hidden(a), 'session override')
            sdk(b, 'storage', 'configure', {'browserPreferences':{'showHidden':True, 'order':'sizeAscending'}})
            activate(sessions[0], b)
            native.wait_for(lambda: has_hidden(b), 'local override')
            assert client.js('return document.querySelector(arguments[0]+" select").value', f'#tab-panel-{b}') == 'sizeAscending'
            activate(sessions[1], c)
            native.wait_for(lambda: has_hidden(c), 'global inheritance across projects')
            assert client.js('return document.querySelector(arguments[0]+" select").value', f'#tab-panel-{c}') == 'nameDescending'
            try:
                sdk(c, 'storage', 'replace', 'globalSettings', 0, 1, {})
            except RuntimeError as error:
                assert 'revision_conflict' in str(error)
            else:
                raise AssertionError('Stale settings overwrite accepted')
            replace(a, 'settings', {})
            activate(sessions[0], a)
            native.wait_for(lambda: has_hidden(a), 'reset restores inheritance')
            mark('Files inherit global defaults across projects, session and tab overrides stay independent, stale writes fail, reset restores inheritance')
            client.click('[aria-label="Session and tab menu"]')
            client.click("//details[@open]//button[normalize-space()='Files' and not(@title)]", 'xpath')
            native.wait_for(lambda: client.js('return !!document.querySelector("dialog[open] select")'), 'shared settings dialog')
            assert client.js('return document.querySelector("dialog[open]").textContent.includes("All projects")')
            native.wait_for(lambda: client.js('return !Array.from(document.querySelectorAll("dialog[open] button")).find(el=>el.textContent.includes("Use global defaults"))?.disabled'), 'settings ready for visual inspection')
            (directory/'files-settings.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
            client.click("//dialog[@open]//button[normalize-space()='Close']", 'xpath')
            mark('Shared settings dialog opens through session menu with global/session scope selection')
            pr = new_tab(sessions[0], 'pr')
            other = new_tab(sessions[0], 'pr')
            replace(pr, 'session', {'repository':'example/repo', 'number':42})
            replace(pr, 'settings', {'connection':'session', 'refreshIntervalSeconds':300})
            sdk(pr, 'credentials', 'set', f'github-session-{sessions[0]}', json.dumps({'token':'second-fixture','login':'second-user'}))
            reload()
            native.wait_for(lambda: 'Connected as second-user' in plugins.pr_connection_text(client, pr), 'session credential overrides environment')
            assert 'Connected as second-user' in plugins.pr_connection_text(client, other)
            sdk(pr, 'credentials', 'set', f'github-tab-{pr}', json.dumps({'token':'first-fixture','login':'fixture-user'}))
            sdk(pr, 'storage', 'configure', {'connection':'tab','pullRequest':{'repository':'local/repo','number':17},'refreshIntervalSeconds':60})
            reload()
            state = client.invoke('sessions')
            session = next(s for s in state['sessions'] if s['id']==sessions[0])
            assert session['pullRequest'] == {'repository':'example/repo','number':42}
            assert next(t for t in session['tabs'] if t['id']==pr)['config']['pullRequest']['number']==17
            assert 'Connected as second-user' in plugins.pr_connection_text(client, other)
            activate(sessions[0], pr)
            client.click('[aria-label="Session and tab menu"]')
            client.click("//details[@open]//button[normalize-space()='Configure…']", 'xpath')
            native.wait_for(lambda: client.js('return document.querySelector("dialog[open] section")?.textContent.includes("Connected as fixture-user")'), 'local account used by tab')
            assert client.js('return document.querySelector("dialog[open] input[type=checkbox]").getBoundingClientRect().width') == 16
            (directory/'pr-local-settings.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
            client.click("//dialog[@open]//label[contains(.,'Use a different pull request for this tab')]//input", 'xpath')
            client.click("//dialog[@open]//button[normalize-space()='Save']", 'xpath')
            native.wait_for(lambda: not client.js('return !!document.querySelector("dialog[open]")'), 'local override reset')
            state = client.invoke('sessions')
            session = next(s for s in state['sessions'] if s['id']==sessions[0])
            assert session['pullRequest'] == {'repository':'example/repo','number':42}
            assert next(t for t in session['tabs'] if t['id']==pr)['config']['pullRequest'] is None
            assert sdk(pr, 'credentials', 'get', 'github-account') is None
            mark('PR session accounts bypass environment tokens; local account works and resetting a local PR preserves the shared association')
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
            native.wait_for(lambda: not any(p['name'].startswith('ongleet') for p in native.owned_processes(directory)), 'Quit', timeout=15)
            try: client.request('DELETE', '')
            except Exception: pass
            client.session = None
            start()
            assert sdk(a, 'storage', 'read', 'globalSettings')['value']['showHidden'] is True
            assert sdk(pr, 'storage', 'read', 'settings')['value']['connection']=='session'
            assert sdk(pr, 'credentials', 'get', f'github-tab-{pr}') is not None
            mark('Global/session settings and scoped credentials survive native Quit/restart')
            report['passed'] = True
        except Exception as error:
            report['error'] = str(error)
            if client and client.session:
                try: (directory/'failure.txt').write_text(client.js('return document.body.innerText'))
                except Exception: pass
            raise
        finally:
            if client and client.session:
                try:
                    client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                    client.request('DELETE', '')
                except Exception: pass
            try: native.wait_for(lambda: not any(p['name'].startswith('ongleet') for p in native.owned_processes(directory)), 'app cleanup', timeout=10)
            except AssertionError: pass
            survivors = [p for p in native.owned_processes(directory) if not p['name'].startswith('weston') and p['name'] not in ['WebKitWebDriver','gnome-keyring-d']]
            report['cleanupSurvivors'] = survivors
            for process in survivors:
                try: os.kill(process['pid'], signal.SIGKILL)
                except ProcessLookupError: pass
            for process in [driver, compositor, keyring]:
                if process:
                    process.terminate()
                    process.wait(timeout=5)
            (directory/'report.json').write_text(json.dumps(report, indent=2))
            print(directory, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application', default=str(native.ROOT/'src-tauri/target/debug/ongleet-gui'))
    run(parser.parse_args().application)
