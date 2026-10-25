"""Exercise the Files editor in isolated native WebKitGTK metadata and fixtures."""
import argparse
import base64
import ctypes
import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time

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
        native.wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), 'restored application')

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
            file.write_text('Automatically refreshed\n')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Automatically refreshed')"), 'automatic clean-file refresh')
            mark('clean editor automatically follows external file changes')
            replacement = folder / 'replacement.tmp'
            for text in ['First atomic replacement\n', 'Second atomic replacement\n']:
                replacement.write_text(text)
                replacement.replace(file)
                native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes(arguments[0])", text.strip()), 'atomic replacement refresh')
            file.unlink()
            native.wait_for(lambda: client.js("return !!document.querySelector('.text-editor [role=alert]')"), 'deleted file error')
            file.write_text('Recreated file\n')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Recreated file') && !document.querySelector('.text-editor [role=alert]')"), 'recreated file refresh')
            # Observe actual filesystem reads; Tauri's invoke property is immutable.
            libc = ctypes.CDLL(None, use_errno=True)
            access_fd = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
            assert access_fd >= 0
            try:
                assert libc.inotify_add_watch(access_fd, os.fsencode(file), 1) >= 0  # IN_ACCESS
                time.sleep(1.5)
                try:
                    os.read(access_fd, 4096)
                except BlockingIOError:
                    pass
                else:
                    raise AssertionError('Idle editor unexpectedly read the file')
                file.write_text('Read counter verified\n')
                native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Read counter verified')"), 'observed notification-driven read')
                assert os.read(access_fd, 4096)  # Positive control for the read observer.
            finally:
                os.close(access_fd)
            mark('inotify survives repeated atomic replacement and recreation, with no idle reads')
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
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')"), 'overwrite confirmation')
            assert client.js("return document.querySelector('.cm-content').innerText.includes('unsaved')")
            client.click("//dialog[@open]//button[normalize-space()='Cancel']", 'xpath')
            assert file.read_text() == 'External change\n'
            button('Save')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')"), 'second overwrite confirmation')
            file.write_text('Changed again while confirming\n')
            client.click("//dialog[@open]//button[normalize-space()='Force save']", 'xpath')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')"), 'renewed overwrite confirmation')
            assert file.read_text() == 'Changed again while confirming\n'
            client.click("//dialog[@open]//button[normalize-space()='Force save']", 'xpath')
            native.wait_for(lambda: 'unsaved' in file.read_text(), 'confirmed overwrite')
            mark('external changes preserve drafts; overwrite requires confirmation for the current revision')
            client.click('.cm-content')
            client.keys(' draft')
            file.write_text('External change\n')
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
            native.wait_for(lambda: client.js("return !document.querySelector('.files-browser').hidden"), 'browser visible before navigation')
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
            draft_key = 'devcenter.editor.draft:' + json.dumps([tab, str(second)], separators=(',', ':'))
            client.click('.cm-content')
            ctrl('a')
            client.keys('Persistent café draft')
            native.wait_for(lambda: client.js("return document.querySelector('.editor-modified-label')?.textContent.includes('Draft saved locally') && JSON.parse(localStorage.getItem(arguments[0])).text === 'Persistent café draft'", draft_key), 'local draft and modified badge')
            assert second.read_text() == 'Restored saved contents\n'
            client.request('POST', '/refresh', {})
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Persistent café draft')"), 'draft restored after WebView reload')
            second.write_text('External update during draft\n')
            restart()
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Persistent café draft') && document.querySelector('.text-editor').textContent.includes('file changed on disk')"), 'draft restored after restart with original revision')
            button('Save')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')"), 'restored draft conflict')
            client.click("//dialog[@open]//button[normalize-space()='Cancel']", 'xpath')
            assert second.read_text() == 'External update during draft\n'
            assert client.js('return localStorage.getItem(arguments[0]) !== null', draft_key)
            button('Save')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')"), 'restored draft overwrite confirmation')
            client.click("//dialog[@open]//button[normalize-space()='Force save']", 'xpath')
            native.wait_for(lambda: second.read_text() == 'Persistent café draft' and client.js('return localStorage.getItem(arguments[0]) === null && !document.querySelector(\".editor-modified-label\")', draft_key), 'saved draft removed and badge cleared')
            mark('local drafts survive reload/restart, retain original revisions and require confirmed overwrite')
            client.click('.cm-content')
            client.keys(' discard me')
            button('← Files')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Unsaved changes\"][open]')"), 'discard draft confirmation')
            client.click("//dialog[@open]//button[normalize-space()='Discard changes']", 'xpath')
            native.wait_for(lambda: client.js('return !document.querySelector(\".cm-content\") && localStorage.getItem(arguments[0]) === null', draft_key), 'discarded draft removed')
            client.click("//button[.//span[text()='resume.txt']]", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Persistent café draft') && !document.querySelector('.editor-modified-label')"), 'discard does not resurrect draft')
            mark('explicit discard removes the local draft and modified badge')
            client.js("window.originalDraftSetItem=Storage.prototype.setItem;Storage.prototype.setItem=function(key,value){if(key.startsWith('devcenter.editor.draft:'))throw new DOMException('Test quota exhausted','QuotaExceededError');return window.originalDraftSetItem.call(this,key,value)}")
            client.click('.cm-content')
            client.keys(' quota')
            native.wait_for(lambda: client.js("return document.querySelector('.text-editor [role=alert]')?.textContent.includes('Could not store') && document.querySelector('.editor-modified-label')?.textContent.includes('unavailable')"), 'draft storage failure is visible')
            assert second.read_text() == 'Persistent café draft'
            client.js('Storage.prototype.setItem=window.originalDraftSetItem')
            client.keys(' recovered')
            native.wait_for(lambda: client.js('return localStorage.getItem(arguments[0]) !== null && !document.querySelector(\".text-editor [role=alert]\")', draft_key), 'draft storage recovers after failure')
            button('Reload')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Unsaved changes\"][open]')"), 'reload draft discard confirmation')
            client.click("//dialog[@open]//button[normalize-space()='Discard changes']", 'xpath')
            native.wait_for(lambda: client.js('return localStorage.getItem(arguments[0]) === null && !document.querySelector(\".editor-modified-label\")', draft_key), 'reload removes draft')
            mark('local storage failure preserves edits, reports unavailable backup and recovers')
            client.click('.cm-content')
            ctrl('a')
            client.keys('Missing file draft')
            second.unlink()
            restart()
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Missing file draft')"), 'draft restored despite missing file')
            assert not second.exists()
            second.write_text('Restored saved contents\n')
            button('Reload')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Unsaved changes\"][open]')"), 'discard missing-file draft')
            client.click("//dialog[@open]//button[normalize-space()='Discard changes']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Restored saved contents') && !document.querySelector('.editor-modified-label')"), 'recreated file loaded')
            mark('drafts remain recoverable when the source file is missing without recreating it')
            second.write_text('Title\nkeep\nBody\nFooter\n')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Footer')"), 'base version for three-way merge')
            client.click('.cm-content')
            ctrl('a')
            client.keys('New title\nkeep\nBody\nFooter\n')
            second.write_text('Title\nkeep\nNew body\nFooter\n')
            button('Save')
            native.wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')?.querySelectorAll('button').length === 3"), 'three conflict actions')
            client.click("//dialog[@open]//button[normalize-space()='Merge into editor']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('New title') && document.querySelector('.cm-content')?.innerText.includes('New body') && document.querySelector('.text-editor').textContent.includes('Review the result')"), 'independent edits merged in native worker')
            assert second.read_text() == 'Title\nkeep\nNew body\nFooter\n'
            assert client.js("return JSON.parse(localStorage.getItem(arguments[0])).baseline.text === arguments[1]", draft_key, 'Title\nkeep\nNew body\nFooter\n')
            ctrl('s')
            native.wait_for(lambda: second.read_text() == 'New title\nkeep\nNew body\nFooter\n', 'reviewed merge saves against new disk revision')
            mark('three-way merge applies independent edits to the editor without saving until requested')
            client.click('.cm-content')
            ctrl('a')
            client.keys('Local competing edit\n')
            second.write_text('Disk competing edit\n')
            button('Save')
            native.wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"File changed on disk\"][open]')"), 'overlapping change conflict')
            client.click("//dialog[@open]//button[normalize-space()='Merge into editor']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('<<<<<<< Local edits') && document.querySelector('.cm-content')?.innerText.includes('>>>>>>> Disk version')"), 'conflict markers in editor')
            assert second.read_text() == 'Disk competing edit\n'
            client.request('POST', '/refresh', {})
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('<<<<<<< Local edits') && document.querySelector('.cm-content')?.innerText.includes('Disk competing edit')"), 'merged conflict draft survives reload')
            client.click('.cm-content')
            ctrl('a')
            client.keys('Manually resolved\n')
            ctrl('s')
            native.wait_for(lambda: second.read_text() == 'Manually resolved\n' and client.js('return !document.querySelector(\".editor-modified-label\")'), 'resolved merge saves normally')
            mark('overlapping edits generate recoverable conflict markers and can be resolved and saved')
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
            # Closing a GUI tab now archives its definition, retaining the editor path.
            second.write_text('Archive saved contents\n')
            client.click('.files-refresh')
            client.click("//button[.//span[text()='resume.txt']]", 'xpath')
            native.wait_for(lambda: client.js("return !!document.querySelector('.cm-content')"), 'editor for archive')
            client.click("button[aria-label='Close Files']")
            native.wait_for(lambda: not any(t['id'] == tab for row in client.invoke('sessions')['sessions'] for t in row['tabs']), 'tab archived')
            archives = client.invoke('archives')['entries']
            assert len(archives) == 1 and archives[0]['kind'] == 'tab'
            archive_id = archives[0]['id']
            client.click("//footer//button[normalize-space()='Archives']", 'xpath')
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Restore']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=Archives]').innerText.includes('No archives.')"), 'archive restored')
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Close']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Archive saved contents')"), 'restored editor')
            assert second.read_text() == 'Archive saved contents\n'
            mark('GUI tab archive restores its stable ID, folder and open editor without changing files')
            client.click("[aria-label='Actions for Editor test']")
            client.click("//button[normalize-space()='Archive session']", 'xpath')
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Confirm']", 'xpath')
            native.wait_for(lambda: not client.invoke('sessions')['sessions'], 'session archived')
            client.click("//footer//button[normalize-space()='Archives']", 'xpath')
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Restore']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('dialog[aria-label=Archives]').innerText.includes('No archives.')"), 'session restored')
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Close']", 'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('Archive saved contents')"), 'session editor restored')
            assert client.invoke('sessions')['sessions'][0]['id'] == session['id']
            mark('GUI session archive and restore retains the original session and plugin state')
            # Permanent deletion is separately confirmed and never deletes working files.
            client.click("button[aria-label='Close Files']")
            native.wait_for(lambda: client.invoke('archives')['entries'], 'second tab archive')
            newest = client.invoke('archives')['entries'][0]['id']
            assert newest > archive_id
            if args.cli:
                def cli(*arguments):
                    result = subprocess.run([str(Path(args.cli).resolve()), '--json', *arguments], env=env, text=True, capture_output=True, timeout=10)
                    return result, json.loads(result.stdout if result.returncode == 0 else result.stderr)
                result, response = cli('archive', 'list')
                assert result.returncode == 0 and response['data']['entries'][0]['id'] == newest
                result, response = cli('archive', 'purge', str(newest))
                assert result.returncode == 5 and response['error']['code'] == 'confirmation_required'
                result, response = cli('archive', 'restore', '999999999')
                assert result.returncode == 4 and response['error']['code'] == 'not_found'
                mark('real private socket CLI lists archives and rejects unconfirmed or missing targets')
            try:
                client.invoke('purge_archive', {'id': newest, 'confirmed': False})
            except RuntimeError as error:
                assert 'confirmation' in str(error).lower()
            else:
                raise AssertionError('Unconfirmed archive purge was accepted')
            restart()
            assert client.invoke('archives')['entries'][0]['id'] == newest
            client.click("//footer//button[normalize-space()='Archives']", 'xpath')
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Delete permanently']", 'xpath')
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Cancel']", 'xpath')
            assert client.invoke('archives')['entries'][0]['id'] == newest
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Delete permanently']", 'xpath')
            client.click("//dialog[@aria-label='Confirm action']//button[normalize-space()='Confirm']", 'xpath')
            native.wait_for(lambda: not client.invoke('archives')['entries'], 'permanent archive deletion')
            assert second.read_text() == 'Archive saved contents\n'
            client.click("//dialog[@aria-label='Archives']//button[normalize-space()='Close']", 'xpath')
            mark('archives survive restart, require confirmation for permanent deletion and preserve working files')
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
    parser.add_argument('--cli')
    run(parser.parse_args())
