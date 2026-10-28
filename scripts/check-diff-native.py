"""Validate Diff in bundled WebKitGTK on a private Xvfb display."""
import argparse
import base64
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
    directory = Path(tempfile.mkdtemp(prefix='ongleet-diff-native-'))
    folder = directory / 'repository'
    folder.mkdir()
    def git(*argv):
        return subprocess.check_output(['git', '-C', str(folder), *argv])
    git('init', '-b', 'main')
    git('config', 'user.email', 'fixture@example.test')
    git('config', 'user.name', 'Fixture')
    file = folder / 'hello.txt'
    file.write_text('before\n')
    second = folder/'z-second.txt'
    second.write_text('second original\n')
    git('add', '.')
    git('commit', '-m', 'base')
    file.write_text('original\n')
    git('add', '.')
    git('commit', '-m', 'next')
    file.write_text('changed ' + 'long line ' * 80 + '\n')
    second.write_text('second changed\n')
    original_index = (folder / '.git/index').read_bytes()
    actions = directory / 'config/ongleet/projects/1/actions.yaml'
    actions.parent.mkdir(parents=True)
    actions.write_text(json.dumps({'actions': [
        {'id': 'shell', 'name': 'Shell', 'kind': 'terminal', 'config': {'executable': '/bin/sleep', 'args': ['600']}},
        {'id': 'diff', 'name': 'Diff', 'kind': 'diff', 'config': {}}
    ]}))
    report = {'artifacts': str(directory), 'checks': []}
    def mark(text):
        report['checks'].append(text)
        print(text, flush=True)
    env = dict(os.environ, XDG_DATA_HOME=str(directory/'data'), XDG_CONFIG_HOME=str(directory/'config'), XDG_CACHE_HOME=str(directory/'cache'), GDK_BACKEND='x11', TAURI_WEBVIEW_AUTOMATION='true', TERMINAL_VALIDATION_RUN=str(directory))
    env.pop('WAYLAND_DISPLAY', None)
    tools = directory/'bin'
    tools.mkdir()
    slow = directory/'slow-git'
    active = directory/'active-git'
    wrapper = tools/'git'
    wrapper.write_text("#!/usr/bin/python3\nimport os,time,pathlib\nroot=pathlib.Path(" + repr(str(directory)) + ")\nif (root/'slow-git').exists():\n (root/'active-git').write_text(str(os.getpid()))\n time.sleep(30)\nos.execv('/usr/bin/git',['git',*__import__('sys').argv[1:]])\n")
    wrapper.chmod(0o755)
    env['PATH'] = str(tools) + ':' + env['PATH']
    client = None
    display = driver = None
    with (directory/'native.log').open('w') as log:
        try:
            read_fd, write_fd = os.pipe()
            display = subprocess.Popen([args.xvfb, '-displayfd', str(write_fd), '-screen', '0', '1280x900x24', '-nolisten', 'tcp'], pass_fds=(write_fd,), stdout=log, stderr=log)
            os.close(write_fd)
            assert select.select([read_fd], [], [], 10)[0]
            env['DISPLAY'] = ':' + os.read(read_fd, 64).decode().strip()
            os.close(read_fd)
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', 0))
                port = listener.getsockname()[1]
            driver = subprocess.Popen([args.driver, '--host=127.0.0.1', f'--port={port}'], env=env, stdout=log, stderr=log)
            client = native.WebDriver(f'http://127.0.0.1:{port}')
            native.wait_for(lambda: client.request('GET','/status',session=False), 'driver')
            def start():
                result = client.request('POST','/session',{'capabilities':{'alwaysMatch':{'webkitgtk:browserOptions':{'binary':str(Path(args.application).resolve())}}}},session=False)
                client.session = result['sessionId']
                native.wait_for(lambda: client.js('return !!document.querySelector(\'[aria-label="Create session"]:not(:disabled)\')'), 'application')
            start()
            client.click('[aria-label="Create session"]')
            client.fill('#session-name','Diff test')
            client.fill('#session-directory',str(folder))
            client.click("//button[normalize-space()='Save session']",'xpath')
            snapshot = native.wait_for(lambda: (s if s['sessions'] else None) if (s := client.invoke('sessions')) else None,'session')
            session = snapshot['sessions'][0]
            created = client.invoke('create_tab',{'sessionId':session['id'],'actionId':'diff'})
            tab = created['sessions'][0]['selectedTabId']
            client.request('POST','/refresh',{})
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('changed')"),'unified patch')
            mark('Manifest discovery, native SDK Git execution, unified rendering')
            assert client.js("const v=document.querySelector('.diff-viewer'); return v.scrollWidth>v.clientWidth")
            assert client.js("const node=document.querySelector('.diff-code-insert'); const range=document.createRange(); range.selectNodeContents(node); const s=getSelection();s.removeAllRanges();s.addRange(range);return s.toString().includes('changed')")
            mark('Long-line overflow and DOM text selection')
            file.write_text('refreshed\n')
            client.click("//section[contains(@class,'diff-panel')]//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'refresh')
            mark('Manual refresh reads current tracked contents')
            client.click("//nav[@aria-label='Changed files']//button[contains(.,'z-second.txt')]",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('second changed')"),'second file')
            client.click("//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelector('.diff-code-insert')?.textContent.includes('second changed')"),'retained selection')
            second.write_text('second original\n')
            client.click("//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'removed selection fallback')
            mark('Refresh retains selection, then selects first file when selected change disappears')

            def mode(value):
                client.js("const el=document.querySelector('.diff-toolbar select'); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(el,arguments[0]);el.dispatchEvent(new Event('change',{bubbles:true}));", value)
            mode('refs')
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('original') && document.querySelector('.diff-summary').textContent.includes('Two refs')"),'two refs')
            mode('mergeBase')
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-summary')?.textContent.includes('Merge base')"),'merge base')
            mark('Inline two-ref and merge-base comparisons render committed changes')
            mode('workingDirectory')
            client.fill('.diff-toolbar label:nth-child(3) input', 'HEAD')
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'working directory restored')
            shell = session['tabs'][0]['id']
            client.click(f'#tab-{shell}')
            file.write_text('activation refreshed\n')
            client.click(f'#tab-{tab}')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('activation refreshed')"),'activation refresh')
            file.write_text('refreshed\n')
            client.js("window.dispatchEvent(new Event('focus'))")
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'focus refresh')
            mark('Tab activation and focus event refresh visible comparison')

            client.js("const inputs=document.querySelectorAll('.diff-toolbar input'); const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set; setter.call(inputs[1],'missing'); inputs[1].dispatchEvent(new Event('input',{bubbles:true}));")
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return !!document.querySelector('.diff-panel [role=alert]')"),'invalid ref')
            assert client.js("return document.querySelector('.diff-code-insert').textContent.includes('refreshed') && document.querySelector('.diff-summary').textContent.includes('Stale')")
            mark('Failed comparison retains successful patch and marks stale')
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
            native.wait_for(lambda: not [p for p in native.owned_processes(directory) if p['name'] != 'WebKitWebDriver'],'quit',timeout=15)
            client.request('DELETE','')
            client.session=None
            start()
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'restart comparison')
            assert client.js("return document.querySelectorAll('.diff-toolbar input')[1].value==='HEAD'")
            mark('Restart restores successful settings and fetches fresh patch')
            assert (folder/'.git/index').read_bytes()==original_index
            assert file.read_text()=='refreshed\n'
            mark('Git index and tracked contents remain unchanged by viewer')
            (directory/'screen.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
            siblings = [p['pid'] for p in native.owned_processes(directory) if p['name']=='sleep']
            assert siblings, 'Expected a live sibling terminal process'
            def gone(pid):
                return not Path(f'/proc/{pid}').exists()
            slow.touch()
            client.click("//section[contains(@class,'diff-panel')]//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: active.exists(), 'slow Git running')
            pid=int(active.read_text())
            client.click(f'#tab-{shell}')
            native.wait_for(lambda: gone(pid),'hidden Git cancellation')
            slow.unlink()
            active.unlink()
            client.click(f'#tab-{tab}')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'visible comparison')
            mark('Hiding cancels and reaps active Git work; returning restores comparison')
            slow.touch()
            client.click("//section[contains(@class,'diff-panel')]//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: active.exists(), 'closing Git running')
            pid=int(active.read_text())
            client.invoke('close_tab',{'id':tab,'confirmed':True})
            native.wait_for(lambda: gone(pid),'closed Git cleanup')
            assert all(Path(f'/proc/{pid}').exists() for pid in siblings)
            mark('Closing during active Git work reaps child and preserves sibling PTY')
        except Exception as error:
            report['error']=str(error)
            if client and client.session:
                try: (directory/'failure.txt').write_text(client.js('return document.body.innerText'))
                except Exception: pass
            raise
        finally:
            if client and client.session:
                try:
                    client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                    client.request('DELETE','')
                except Exception: pass
            try: native.wait_for(lambda: not [p for p in native.owned_processes(directory) if p['name'] != 'WebKitWebDriver'],'cleanup',timeout=10)
            except AssertionError: pass
            report['cleanupSurvivors']=[p for p in native.owned_processes(directory) if p['name'] != 'WebKitWebDriver']
            for process in report['cleanupSurvivors']:
                try: os.kill(process['pid'],signal.SIGKILL)
                except ProcessLookupError: pass
            for process in [driver,display]:
                if process:
                    process.terminate()
                    process.wait(timeout=5)
            (directory/'report.json').write_text(json.dumps(report,indent=2))
            print(directory,flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['application','driver','xvfb']: parser.add_argument('--'+name,required=True)
    run(parser.parse_args())
