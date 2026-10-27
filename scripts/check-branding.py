"""Check a real DevCenter WebKit profile upgrade on a private display and bus.

Requires a saved pre-rename bundled GUI, current bundled GUI/CLI, Xvfb and
WebKitWebDriver. Run under dbus-run-session. Never opens personal metadata.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location('native', Path(__file__).with_name('check-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def run(args):
    assert '/tmp/' in os.environ.get('DBUS_SESSION_BUS_ADDRESS', ''), 'Use dbus-run-session'
    root = Path(tempfile.mkdtemp(prefix='ongleet-branding-native-'))
    env = dict(os.environ, XDG_DATA_HOME=str(root / 'data'), XDG_CONFIG_HOME=str(root / 'config'),
               XDG_CACHE_HOME=str(root / 'cache'), TAURI_WEBVIEW_AUTOMATION='true',
               GDK_BACKEND='x11', TERMINAL_VALIDATION_RUN=str(root))
    checks = []
    cgroups = set()
    processes = []
    client = None
    def mark(message):
        checks.append(message)
        print('PASS:', message, flush=True)
    with (root / 'native.log').open('w') as log:
        try:
            read, write = os.pipe()
            display = subprocess.Popen([args.xvfb, '-displayfd', str(write), '-screen', '0', '1100x760x24', '-nolisten', 'tcp'], pass_fds=(write,), stdout=log, stderr=log)
            processes.append(display)
            os.close(write)
            assert select.select([read], [], [], 10)[0]
            env['DISPLAY'] = ':' + os.read(read, 64).decode().strip()
            os.close(read)
            subprocess.run(['dbus-update-activation-environment', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'DISPLAY'], env=env, check=True)
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0))
                port = sock.getsockname()[1]
            driver = subprocess.Popen([args.driver, '--host=127.0.0.1', f'--port={port}'], env=env, stdout=log, stderr=log)
            processes.append(driver)
            client = native.WebDriver(f'http://127.0.0.1:{port}')
            native.wait_for(lambda: client.request('GET', '/status', session=False), 'driver')
            def launch(binary):
                result = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {'webkitgtk:browserOptions': {'binary': str(Path(binary).resolve())}}}}, session=False)
                client.session = result['sessionId']
                try:
                    native.wait_for(lambda: client.js("return !!window.__TAURI_INTERNALS__ && !!document.querySelector('[aria-label=\"Create session\"]')"), 'application')
                except Exception:
                    (root / 'page.json').write_text(json.dumps(client.js('return {url:location.href,title:document.title,body:document.body.innerText}')))
                    raise
            def quit_app():
                for process in native.owned_processes(root):
                    try:
                        for line in Path(f"/proc/{process['pid']}/cgroup").read_text().splitlines():
                            if line.startswith('0::') and any(prefix in line for prefix in ('/devcenter-', '/ongleet-')):
                                cgroups.add('/sys/fs/cgroup' + line[3:])
                    except FileNotFoundError:
                        pass
                client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                native.wait_for(lambda: not any(p['name'] in ('devcenter-gui','ongleet-gui') for p in native.owned_processes(root)), 'GUI exit', timeout=10)
                client.request('DELETE', '')
                client.session = None
            file = root / 'draft.txt'
            file.write_text('Original contents')
            config = root / 'config/devcenter/projects/1'
            config.mkdir(parents=True)
            (config / 'actions.yaml').write_text(json.dumps({'actions': [
                {'id':'sleep','name':'Sleep','kind':'terminal','config':{'executable':'/bin/sleep','args':['300']}},
                {'id':'files','name':'Files','kind':'files','config':{'folder':str(root),'editorFile':str(file)}},
            ]}))
            launch(args.legacy_application)
            created = client.invoke('create_session', {'name':'Branding fixture','directory':str(root),'projectId':None})
            session = created['sessions'][0]['id']
            created = client.invoke('create_tab', {'sessionId':session,'actionId':'files'})
            tab = created['sessions'][0]['selectedTabId']
            client.request('POST', '/refresh', {})
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Original contents')"), 'legacy editor')
            client.click('.cm-content')
            client.js("document.querySelector('.cm-content').dispatchEvent(new KeyboardEvent('keydown',{key:'a',code:'KeyA',ctrlKey:true,bubbles:true,cancelable:true}))")
            client.keys('Recovered café draft')
            draft_key = 'devcenter.editor.draft:' + json.dumps([tab,str(file)], separators=(',',':'))
            draft = native.wait_for(lambda: client.js('return localStorage.getItem(arguments[0])',draft_key), 'legacy local draft')
            values = {
                draft_key: draft,
                'devcenter.editor.wrap': 'true',
                'devcenter.sidebarWidth': '280',
                'devcenter.editor.draft:[42,"/missing/draft.txt"]': json.dumps({'version':1,'baseline':{'path':'/missing/draft.txt','text':'before','revision':'fixture','bom':False,'lineEnding':'LF'},'text':'recovered draft'}),
                'devcenter.pr-reviews.v1:7:example/repo#1:viewer': '{"review":"acknowledged"}',
            }
            client.js('for(const [k,v] of Object.entries(arguments[0])) localStorage.setItem(k,v)', values)
            quit_app()
            mark('Legacy WebKitGTK profile persists draft, wrap and PR preference fixtures')
            launch(args.application)
            for old, value in values.items():
                new = old.replace('devcenter', 'ongleet', 1)
                assert client.js('return localStorage.getItem(arguments[0])', new) == value, new
                assert client.js('return localStorage.getItem(arguments[0])', old) is None, old
            mark('Tauri identity upgrade preserves browser values and removes copied legacy keys')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Recovered café draft') && !!document.querySelector('.cm-lineWrapping')"), 'recovered draft and wrap consumer')
            assert file.read_text() == 'Original contents'
            mark('Actual Files editor restores unsaved draft and wrapping without changing disk contents')
            snapshot = client.invoke('sessions')
            assert snapshot['sessions'][0]['id'] == session and snapshot['sessions'][0]['selectedTabId'] == tab
            client.invoke('close_after_delay', {'confirmed':True})
            result = subprocess.run([str(Path(args.application).resolve()), '--show'], env=env, capture_output=True, text=True, timeout=10)
            assert result.returncode == 0, result.stderr
            mark('Hide/reopen through new executable preserves session and selected tab')
            # Read the actual editor consumer, which initializes its preference at import time.
            assert client.js("return document.title.includes('Ongleet')")
            mark('New application identity renders in native WebKitGTK')
            quit_app()
            launch(args.application)
            assert client.js("return localStorage.getItem('ongleet.editor.wrap')") == 'true'
            mark('Second native launch retains migrated preferences')
            quit_app()
        finally:
            if client and client.session:
                try:
                    client.invoke('quit', {'confirmed': True})
                    client.request('DELETE', '')
                except Exception:
                    pass
            for process in reversed(processes):
                process.terminate()
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            # WebKit child teardown can finish shortly after the GUI/driver exit.
            try:
                native.wait_for(lambda: not native.owned_processes(root), 'native child cleanup', timeout=5)
            except AssertionError:
                pass
            survivors = native.owned_processes(root)
            remaining_cgroups = [path for path in cgroups if Path(path).exists()]
            (root / 'report.json').write_text(json.dumps({'checks': checks, 'artifacts': str(root), 'survivors':survivors, 'remainingCgroups':remaining_cgroups}, indent=2))
            print('Report:', root / 'report.json', flush=True)
            for process in survivors:
                try: os.kill(process['pid'], signal.SIGKILL)
                except ProcessLookupError: pass
            assert not survivors, survivors
            assert not remaining_cgroups, remaining_cgroups


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['legacy-application', 'application', 'driver', 'xvfb']:
        parser.add_argument('--' + name, required=True)
    run(parser.parse_args())
