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
    (folder/'nested').mkdir()
    second = folder/'nested/z-second.txt'
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
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('original')"),'automatic latest commit')
            assert client.js("return !Array.from(document.querySelectorAll('.diff-toolbar label')).some(el=>el.textContent.includes('Repository folder'))")
            mark('Automatic latest-commit default and repository settings absent from toolbar')
            initial_width = client.js("return Number(document.querySelector('[aria-label=\"Resize diff sidebar\"]').getAttribute('aria-valuenow'))")
            point = client.js("const r=document.querySelector('.diff-sidebar-resizer').getBoundingClientRect(); return {x:Math.round(r.x+r.width/2),y:Math.round(r.y+40)}")
            client.request('POST','/actions',{'actions':[{'type':'pointer','id':'resize-mouse','parameters':{'pointerType':'mouse'},'actions':[
                {'type':'pointerMove','duration':0,'x':point['x'],'y':point['y']},
                {'type':'pointerDown','button':0},
                {'type':'pointerMove','duration':150,'x':point['x']+50,'y':point['y']},
                {'type':'pointerUp','button':0}
            ]}]})
            native.wait_for(lambda: client.js("return Number(localStorage.getItem('ongleet.diff.sidebarWidth'))") == initial_width+50, 'saved drag width')
            assert client.js("return !document.querySelector('.diff-sidebar-resize-overlay')")
            client.request('POST','/refresh',{})
            native.wait_for(lambda: client.js("return Number(document.querySelector('.diff-sidebar-resizer')?.getAttribute('aria-valuenow'))") == initial_width+50, 'restored sidebar width')
            mark('Native pointer drag resizes Diff sidebar; localStorage restores width after reload')

            def mode(value):
                client.js("const el=document.querySelector('.diff-toolbar select'); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(el,arguments[0]);el.dispatchEvent(new Event('change',{bubbles:true}));", value)
            mode('workingDirectory')
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('changed')"),'unified patch')
            mark('Manifest discovery, native SDK Git execution, unified rendering')
            filter_layout = client.js("""
                const aside=document.querySelector('.diff-sidebar aside');
                const input=aside.querySelector('input');
                const nav=aside.querySelector('nav');
                const filler=document.createElement('div');
                filler.style.minHeight='3000px';
                nav.append(filler);
                const before=input.getBoundingClientRect();
                nav.scrollTop=500;
                const after=input.getBoundingClientRect();
                const fits=after.right < aside.getBoundingClientRect().right;
                const fixed=before.top===after.top && nav.scrollTop===500 && aside.scrollTop===0;
                filler.remove();
                nav.scrollTop=0;
                return {fixed, fits, before:before.top, after:after.top, right:after.right, asideRight:aside.getBoundingClientRect().right};
            """)
            assert filter_layout['fixed'] and filter_layout['fits'], filter_layout
            mark('Filename filter stays fixed while tree scrolls; input right border fits inside sidebar')

            assert client.js("const v=document.querySelector('.diff-file-content'); return v.scrollWidth>v.clientWidth")
            assert client.js("const node=document.querySelector('.diff-code-insert'); const range=document.createRange(); range.selectNodeContents(node); const s=getSelection();s.removeAllRanges();s.addRange(range);return s.toString().includes('changed')")
            assert client.js("const file=document.querySelector('.diff-file');const viewer=document.querySelector('.diff-viewer');const r=file.getBoundingClientRect();const v=viewer.getBoundingClientRect();return r.right < v.right && viewer.scrollWidth===viewer.clientWidth")
            mark('Long-line overflow and DOM text selection; right card border stays inside viewer')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]')"), 'patches loaded')
            assert client.js(f"return document.querySelector('#tab-{tab} .tab-name').textContent==='Diff (2)'")
            tags = client.invoke('sessions')['sessions'][0]['tags']
            assert any(tag['name'] == f'Diff #{tab}: 2 files' for tag in tags), tags
            client.click('.diff-file .diff-viewed input')
            assert client.js("const file=document.querySelector('.diff-file'); return file.querySelector('input').checked && !file.querySelector('.diff-file-content')")
            client.click('.diff-file .diff-fold')
            assert client.js("const file=document.querySelector('.diff-file'); return file.querySelector('input').checked && !!file.querySelector('.diff-file-content')")
            client.click('.diff-file .diff-fold')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-file:first-child .diff-file-content')"), 'collapsed viewed file')
            client.click("//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelector('.diff-file input')?.checked"), 'viewed marker retained')
            assert client.js("return !document.querySelector('.diff-file:first-child .diff-file-content')")
            mark('Viewed folds patch; independent expansion and unchanged refresh retain review state')

            file.write_text('refreshed\n')
            client.click("//section[contains(@class,'diff-panel')]//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'refresh')
            assert client.js("return !document.querySelector('.diff-file input').checked && !!document.querySelector('.diff-file:first-child .diff-file-content')")
            mark('Changed patch clears viewed marker, expands file and displays fresh contents')
            client.click('.diff-layout nav summary')
            assert client.js("return !document.querySelector('.diff-layout nav details').open")
            client.click('.diff-layout nav summary')
            client.click("//nav[@aria-label='Changed files']//button[contains(.,'z-second.txt')]",'xpath')
            assert client.js("return document.querySelector('.diff-layout nav button[aria-pressed=true]').textContent.includes('z-second.txt')")
            native.wait_for(lambda: client.js("return document.querySelectorAll('.diff-file').length===2 && document.querySelectorAll('.diff-code-insert')[1]?.textContent.includes('second changed')"),'second file')
            client.click("//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelectorAll('.diff-code-insert')[1]?.textContent.includes('second changed')"),'retained selection')
            second.write_text('second original\n')
            client.click("//button[normalize-space()='Refresh']",'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelector('.diff-code-insert')?.textContent.includes('refreshed')"),'removed selection fallback')
            assert client.js(f"return document.querySelector('#tab-{tab} .tab-name').textContent==='Diff (1)'")
            tags = client.invoke('sessions')['sessions'][0]['tags']
            assert any(tag['name'] == f'Diff #{tab}: 1 file' for tag in tags), tags
            assert not any(tag['name'] == f'Diff #{tab}: 2 files' for tag in tags), tags
            mark('Continuous page and tree navigation; tab and session changed-file counts update from two to one')

            def mode(value):
                client.js("const el=document.querySelector('.diff-toolbar select'); Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(el,arguments[0]);el.dispatchEvent(new Event('change',{bubbles:true}));", value)
            mode('refs')
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return document.querySelector('.diff-code-insert')?.textContent.includes('original') && document.querySelector('.diff-summary').textContent.includes('Two refs')"),'two refs')
            mode('mergeBase')
            client.click("//button[normalize-space()='Compare']",'xpath')
            native.wait_for(lambda: client.js("return !document.querySelector('.diff-panel [role=status]') && document.querySelector('.diff-summary')?.textContent.includes('Merge base')"),'merge base')
            mark('Inline two-ref and merge-base comparisons render committed changes')
            mode('workingDirectory')
            client.js("const el=document.querySelector('.diff-toolbar input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,'HEAD');el.dispatchEvent(new Event('input',{bubbles:true}));")
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

            client.js("const inputs=document.querySelectorAll('.diff-toolbar input'); const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set; setter.call(inputs[0],'missing'); inputs[0].dispatchEvent(new Event('input',{bubbles:true}));")
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
            assert client.js("return document.querySelectorAll('.diff-toolbar input')[0].value==='HEAD'")
            mark('Restart restores successful settings and fetches fresh patch')
            assert client.js("return Number(document.querySelector('.diff-sidebar-resizer').getAttribute('aria-valuenow'))") == initial_width+50
            client.click('.diff-sidebar-resizer')
            client.request('POST','/actions',{'actions':[{'type':'key','id':'resize-keyboard','actions':[
                {'type':'keyDown','value':'\ue012'}, {'type':'keyUp','value':'\ue012'}
            ]}]})
            native.wait_for(lambda: client.js("return Number(localStorage.getItem('ongleet.diff.sidebarWidth'))") == initial_width+40, 'keyboard resize')
            mark('Sidebar width survives application restart and supports keyboard resizing')

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
