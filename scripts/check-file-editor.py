"""Exercise the Files editor in isolated native WebKitGTK metadata and fixtures."""
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

spec = importlib.util.spec_from_file_location('native', Path(__file__).with_name('check-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def run(args):
    directory = Path(tempfile.mkdtemp(prefix='devcenter-editor-validation-'))
    config = directory / 'config/devcenter/projects/1'
    config.mkdir(parents=True)
    folder = directory / 'files'
    folder.mkdir()
    file = folder / 'hello.txt'
    file.write_text('Hello editor\n')
    (config / 'actions.yaml').write_text(json.dumps({'actions': [
        {'id': 'shell', 'name': 'Shell', 'kind': 'terminal', 'config': {'executable': '/bin/true', 'args': []}},
        {'id': 'files', 'name': 'Files', 'kind': 'files', 'config': {}},
    ]}))
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
    env = dict(os.environ, XDG_DATA_HOME=str(directory / 'data'), XDG_CONFIG_HOME=str(directory / 'config'),
               XDG_CACHE_HOME=str(directory / 'cache'), TAURI_WEBVIEW_AUTOMATION='true',
               GDK_BACKEND='x11', TERMINAL_VALIDATION_RUN=str(directory))
    client = native.WebDriver(f'http://127.0.0.1:{port}')
    checks = []
    report = {'artifacts': str(directory), 'checks': checks}
    def mark(name):
        checks.append(name)
        print(name, flush=True)
    def ctrl(key):
        # WebKitGTK WebDriver remaps Ctrl+letters through the host keyboard layout
        # (observed a->b, z->m, s->u). Dispatch the intended shortcut to the real
        # CodeMirror DOM; ordinary Unicode typing still uses native key actions.
        client.js("document.querySelector('.cm-content').dispatchEvent(new KeyboardEvent('keydown', {key:arguments[0],code:'Key'+arguments[0].toUpperCase(),ctrlKey:true,bubbles:true,cancelable:true}))", key)
    def button(label):
        if not client.js("return !!document.querySelector('.editor-menu:popover-open')"):
            client.click('.editor-toolbar button[popovertarget]')
        client.click(f"//div[@role='menu' and @popover]//button[starts-with(normalize-space(),'{label}')]", 'xpath')
    def restart():
        client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
        native.wait_for(lambda: not any(process['name'].startswith('devcenter') for process in native.owned_processes(directory)), 'application stopped', timeout=10)
        try:
            client.request('DELETE', '')
        except Exception:
            pass
        client.session = None
        result = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {
            'webkitgtk:browserOptions': {'binary': str(Path(args.application).resolve())}
        }}}, session=False)
        client.session = result['sessionId']
        native.wait_for(lambda: client.js("return !!document.querySelector('.files-browser')"), 'restored Files tab')

    with (directory / 'driver.log').open('w') as log:
        driver = subprocess.Popen([args.driver, '--host=127.0.0.1', f'--port={port}'], env=env,
                                  stdout=log, stderr=log, start_new_session=True)
        try:
            native.wait_for(lambda: client.request('GET', '/status', session=False), 'driver')
            result = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {
                'webkitgtk:browserOptions': {'binary': str(Path(args.application).resolve())}
            }}}, session=False)
            client.session = result['sessionId']
            native.wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), 'application')
            client.click('[aria-label="Create session"]')
            client.fill('#session-name', 'Editor test')
            client.fill('#session-directory', str(folder))
            client.click("//button[normalize-space()='Save session']", 'xpath')
            snapshot = native.wait_for(lambda: (value if value['sessions'] else None) if (value := client.invoke('sessions')) else None, 'session metadata')
            session = snapshot['sessions'][0]
            created = client.invoke('create_tab', {'sessionId': session['id'], 'actionId': 'files'})
            tab = created['sessions'][0]['selectedTabId']
            client.request('POST', '/refresh', {})
            client.click(f"//section[@id='tab-panel-{tab}']//button[.//span[text()='hello.txt']]", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Hello editor')"), 'file editor')
            assert client.js("return !!document.querySelector('.files-browser[hidden]')")
            mark('text opens in place of file browser')
            client.js("window.editorKeys=[];document.addEventListener('keydown',e=>window.editorKeys.push({key:e.key,code:e.code,ctrl:e.ctrlKey,shift:e.shiftKey,target:e.target.className}),true)")
            client.click('.cm-content')
            ctrl('a')
            client.keys('Changed café\nsecond line')
            native.wait_for(lambda: client.js("return document.querySelector('.editor-status').textContent.includes('Unsaved')"), 'dirty indicator')
            before_undo = client.js("return document.querySelector('.cm-content').innerText")
            ctrl('z')
            assert client.js("return document.querySelector('.cm-content').innerText") != before_undo
            button('Redo')
            assert client.js("return document.querySelector('.cm-content').innerText") == before_undo
            button('Find / replace')
            assert client.js("return !!document.querySelector('.cm-search')")
            client.fill('.cm-search input[name=search]', 'second')
            client.fill('.cm-search input[name=replace]', 'next')
            client.click('.cm-search button[name=replaceAll]')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content').innerText.includes('next line')"), 'replace all')
            button('Wrap long lines')
            assert client.js("return !!document.querySelector('.cm-lineWrapping')")
            client.click('.cm-content')
            ctrl('s')
            native.wait_for(lambda: file.read_text() == 'Changed café\nnext line', 'Ctrl+S save')
            mark('native Unicode typing, undo/redo, find panel and DOM Ctrl+S save')
            client.click('.cm-content')
            client.keys(' unsaved')
            button('← Files')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Unsaved changes\"][open]')"), 'discard dialog')
            client.click("//dialog[@open]//button[normalize-space()='Cancel']", 'xpath')
            assert client.js("return !!document.querySelector('.cm-content')")
            mark('back navigation protects unsaved changes')
            client.click(f"button[aria-label='Close Files']")
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), 'tab close guard')
            client.click("//dialog[@open]//button[normalize-space()='Cancel']", 'xpath')
            assert client.js("return !!document.querySelector('.cm-content')")
            client.click("//button[normalize-space()='Quit']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=\"Confirm action\"][open]')?.textContent.includes('unsaved')"), 'Quit guard')
            client.click("//dialog[@open]//button[normalize-space()='Cancel']", 'xpath')
            mark('GUI tab closure and Quit protect unsaved edits')
            client.click(f"#tab-{session['tabs'][0]['id']}")
            native.wait_for(lambda: client.js("return document.querySelector(arguments[0]).getAttribute('aria-selected') === 'true'", f"#tab-{session['tabs'][0]['id']}"), 'sibling selected')
            client.click(f'#tab-{tab}')
            native.wait_for(lambda: client.js("return document.querySelector(arguments[0]).getAttribute('aria-selected') === 'true' && document.querySelector('.cm-content').getBoundingClientRect().height > 0", f'#tab-{tab}'), 'editor visible again')
            assert client.js("return document.querySelector('.cm-content').innerText.includes('unsaved')")
            mark('switching tabs retains in-memory edits and wrap selection')
            (directory / 'editor.png').write_bytes(base64.b64decode(client.request('GET', '/screenshot')))
            file.write_text('External change\n')
            button('Save')
            native.wait_for(lambda: client.js("return document.querySelector('.text-editor [role=alert]')?.textContent.includes('changed on disk')"), 'conflict error')
            assert file.read_text() == 'External change\n'
            mark('external modification blocks overwrite and retains editor')
            button('Reload')
            client.click("//dialog[@open]//button[normalize-space()='Discard changes']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content').innerText.includes('External change')"), 'reload')
            button('← Files')
            native.wait_for(lambda: client.js("return !document.querySelector('.files-browser').hidden"), 'browser restored')
            mark('reload and return to existing folder')
            # The setting must survive a WebView reload and apply to a new editor.
            client.request('POST', '/refresh', {})
            client.click(f"//section[@id='tab-panel-{tab}']//button[.//span[text()='hello.txt']]", 'xpath')
            native.wait_for(lambda: client.js("return !!document.querySelector('.cm-lineWrapping')"), 'persisted wrap setting')
            client.click('.editor-toolbar button[popovertarget]')
            native.wait_for(lambda: client.js("return !!document.querySelector('.editor-menu:popover-open')"), 'actions menu')
            assert client.js("return document.querySelector('[role=menuitemcheckbox]').getAttribute('aria-checked') === 'true'")
            client.keys('\ue015')
            assert client.js("return document.activeElement.textContent.trim() === 'Reload'")
            client.keys('\ue00c')
            native.wait_for(lambda: client.js("return !document.querySelector('.editor-menu:popover-open')"), 'Escape menu dismissal')
            client.click('.editor-toolbar button[popovertarget]')
            client.click('.editor-path')
            native.wait_for(lambda: client.js("return !document.querySelector('.editor-menu:popover-open')"), 'outside-click menu dismissal')
            mark('actions menu keyboard/dismissal and wrapping preference survive WebView reload')
            button('← Files')
            nested = folder / 'nested'
            nested.mkdir()
            second = nested / 'resume.txt'
            second.write_text('Restored saved contents\n')
            client.click('.files-refresh')
            client.click("//button[.//span[text()='nested']]", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('input[aria-label=Folder]').value === arguments[0]", str(nested)), 'nested folder')
            client.click("//button[.//span[text()='resume.txt']]", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Restored saved contents')"), 'second editor')
            restart()
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Restored saved contents')"), 'file restored after stop/start')
            assert client.js("return document.querySelector('input[aria-label=Folder]').value === arguments[0]", str(nested))
            mark('stop/start restores nested folder and open text file')
            button('← Files')
            native.wait_for(lambda: client.js("return !document.querySelector('.cm-content')"), 'editor closed')
            restart()
            native.wait_for(lambda: client.js("return document.querySelector('input[aria-label=Folder]').value === arguments[0]", str(nested)), 'browser folder restored')
            assert not client.js("return !!document.querySelector('.cm-content')")
            mark('returning to Files persists browser mode across restart')
            client.click("//button[.//span[text()='resume.txt']]", 'xpath')
            native.wait_for(lambda: client.js("return !!document.querySelector('.cm-content')"), 'editor reopened')
            second.unlink()
            restart()
            native.wait_for(lambda: client.js("return document.querySelector('.files-browser [role=alert]')?.textContent.includes('Could not reopen')"), 'missing saved file error')
            assert not second.exists()
            client.click("//button[normalize-space()='Keep browsing']", 'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.files-browser [role=alert]')"), 'missing-file recovery')
            mark('missing restored file reports an error without recreation and allows browsing')
            (directory / 'success.png').write_bytes(base64.b64decode(client.request('GET', '/screenshot')))
        except Exception as error:
            report['error'] = str(error)
            if client.session:
                try:
                    (directory / 'keys.json').write_text(json.dumps(client.js('return window.editorKeys')))
                    (directory / 'failure.txt').write_text(client.js('return document.body.innerText'))
                    (directory / 'failure.png').write_bytes(base64.b64decode(client.request('GET', '/screenshot')))
                except Exception:
                    pass
            raise
        finally:
            if client.session:
                try:
                    client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                    client.request('DELETE', '')
                except Exception:
                    pass
            driver.terminate()
            try:
                driver.wait(timeout=5)
            except subprocess.TimeoutExpired:
                driver.kill()
                driver.wait()
            try:
                native.wait_for(lambda: not native.owned_processes(directory), 'isolated app exit', timeout=5)
            except AssertionError:
                pass
            survivors = native.owned_processes(directory)
            report['cleanupSurvivors'] = survivors
            for process in survivors:
                try:
                    os.kill(process['pid'], signal.SIGKILL)
                except ProcessLookupError:
                    pass
            (directory / 'report.json').write_text(json.dumps(report, indent=2))
            print(directory, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--driver', required=True)
    parser.add_argument('--application', required=True)
    run(parser.parse_args())
