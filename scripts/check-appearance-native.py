"""Appearance acceptance in private Weston Wayland/XWayland or Xvfb displays."""
import argparse
import base64
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tempfile
import time

spec = importlib.util.spec_from_file_location('native', Path(__file__).with_name('check-native.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


def run(args):
    directory = Path(tempfile.mkdtemp(prefix='ongleet-appearance-native-'))
    folder = directory / 'repository'
    folder.mkdir()
    (folder/'sample.js').write_text('const greeting = "Hello theme";\nconsole.log(greeting);\n')
    def git(*argv):
        subprocess.run(['git', '-C', str(folder), *argv], check=True, capture_output=True)
    git('init','-b','main')
    git('config','user.email','fixture@example.test')
    git('config','user.name','Fixture')
    git('add','.')
    git('commit','-m','Fixture baseline')
    (folder/'sample.js').write_text('const greeting = "Hello changed theme";\nconsole.log(greeting);\n')
    git('add','.')
    git('commit','-m','Fixture change')
    probe = directory/'output.py'
    probe.write_text('import os,time\nprint("THEME_PID="+str(os.getpid()),flush=True)\ni=0\nwhile True:\n print("theme-output-"+str(i),flush=True)\n i+=1\n time.sleep(.1)\n')
    actions = directory/'config/ongleet/projects/1/actions.yaml'
    actions.parent.mkdir(parents=True)
    actions.write_text(json.dumps({'actions':[
        {'id':'stream','name':'Stream','kind':'terminal','config':{'executable':'/usr/bin/python3','args':[str(probe)]}},
        {'id':'files','name':'Files','kind':'files','config':{}},
        {'id':'diff','name':'Diff','kind':'diff','config':{}},
        *([{'id':'pr','name':'Pull request','kind':'pull-request','config':{}},
           {'id':'archive','name':'Archive fixture','kind':'terminal','config':{'executable':'/bin/true','args':[]}}] if args.surfaces else [])
    ]}))
    runtime = directory/'runtime'
    runtime.mkdir(mode=0o700)
    env = dict(os.environ, XDG_DATA_HOME=str(directory/'data'), XDG_CONFIG_HOME=str(directory/'config'), XDG_CACHE_HOME=str(directory/'cache'), XDG_RUNTIME_DIR=str(runtime), TAURI_WEBVIEW_AUTOMATION='true', TERMINAL_VALIDATION_RUN=str(directory))
    env.pop('DISPLAY',None)
    env.pop('WAYLAND_DISPLAY',None)
    if args.surfaces:
        env['GH_TOKEN']='first-fixture'
        env.pop('GITHUB_TOKEN',None)
    report = {'backend':args.backend,'artifacts':str(directory),'checks':[]}
    client = None
    compositor = driver = None
    def mark(text):
        report['checks'].append(text)
        print(text,flush=True)
    with (directory/'native.log').open('w') as log:
        try:
            if args.backend == 'xvfb':
                read_fd, write_fd = os.pipe()
                compositor = subprocess.Popen(['Xvfb','-displayfd',str(write_fd),'-screen','0','1280x900x24','-nolisten','tcp'],pass_fds=(write_fd,),env=env,stdout=log,stderr=log)
                os.close(write_fd)
                import select
                assert select.select([read_fd],[],[],10)[0]
                env['DISPLAY']=':'+os.read(read_fd,64).decode().strip()
                os.close(read_fd)
                env['GDK_BACKEND']='x11'
            else:
                compositor = subprocess.Popen(['weston','--backend=headless','--renderer=pixman','--width=1280','--height=900','--socket=appearance-wayland','--idle-time=0','--no-config','--xwayland',f'--log={directory}/weston.log'],env=env,stdout=log,stderr=log)
                native.wait_for(lambda:(runtime/'appearance-wayland').exists(),'private Wayland compositor')
                env['WAYLAND_DISPLAY']='appearance-wayland'
                env['GDK_BACKEND']='wayland' if args.backend=='wayland' else 'x11'
                if args.backend=='xwayland':
                    match=native.wait_for(lambda:re.search(r'xserver listening on display (:\d+)',(directory/'weston.log').read_text(),re.I),'private XWayland display')
                    env['DISPLAY']=match[1]
            with socket.socket() as listener:
                listener.bind(('127.0.0.1',0))
                port=listener.getsockname()[1]
            driver=subprocess.Popen(['WebKitWebDriver','--host=127.0.0.1',f'--port={port}'],env=env,stdout=log,stderr=log)
            client=native.WebDriver(f'http://127.0.0.1:{port}')
            native.wait_for(lambda:client.request('GET','/status',session=False),'WebDriver')
            def start():
                result=client.request('POST','/session',{'capabilities':{'alwaysMatch':{'webkitgtk:browserOptions':{'binary':str(Path(args.application).resolve())}}}},session=False)
                client.session=result['sessionId']
                native.wait_for(lambda:client.js('return !!document.querySelector(\'[aria-label="Create session"]:not(:disabled)\')'),'app')
            def button(text):
                client.click(f"//button[normalize-space()='{text}']",'xpath')
            def appearance():
                client.click('.titlebar-menu > summary')
                button('Appearance…')
                native.wait_for(lambda:client.js('return !!document.querySelector("#appearance-title")'),'appearance dialog')
            def choose(index,value):
                client.js("const el=document.querySelectorAll('dialog select')[arguments[0]]; el.value=arguments[1]; el.dispatchEvent(new Event('change',{bubbles:true}));",index,value)
                native.wait_for(lambda:client.js("const p=JSON.parse(localStorage.getItem('ongleet.appearance'));return (arguments[0]===0?p.application:(p.terminal||''))===arguments[1]",index,value),'saved selection')
            def new_tab(name):
                client.click('[aria-label="Session and tab menu"]')
                client.click(f"//details[@open]//button[@title='Open a new tab'][normalize-space()='{name}']",'xpath')
                return client.invoke('sessions')['sessions'][0]['selectedTabId']
            def screenshot(name):
                time.sleep(0.2)  # Let the compositor present the closed dialog and palette.
                (directory/f'{name}.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
            start()
            client.click('[aria-label="Create session"]')
            client.fill('#session-name','Appearance test')
            client.fill('#session-directory',str(folder))
            button('Save session')
            state=native.wait_for(lambda:client.invoke('sessions')['sessions'] or None,'session')
            session=state[0]
            stream=session['selectedTabId']
            native.wait_for(lambda:'THEME_PID=' in client.screen(stream),'stream output')
            shell_pid=int(re.search(r'THEME_PID=(\d+)',client.screen(stream))[1])
            original_runtime=client.state(stream)['sessionId']
            appearance()
            assert client.js("return document.querySelectorAll('dialog select')[0].value==='original' && document.querySelectorAll('dialog select')[1].value===''")
            for theme in ['original','dark-plus','light-plus','darcula','one-dark','monokai']:
                choose(0,theme)
                expected='light' if theme=='light-plus' else 'dark'
                assert client.js('return document.documentElement.style.colorScheme',)==expected
                assert client.state(stream)['sessionId']==original_runtime
                assert Path(f'/proc/{shell_pid}').exists()
                screenshot('appearance-'+theme)
            choose(1,'light-plus')
            choose(0,'one-dark')
            assert client.js("return document.querySelectorAll('.appearance-preview')[1].style.backgroundColor==='rgb(255, 255, 255)'")
            choose(1,'')
            button('Done')
            mark('All six presets switch during live output; terminal runtime/PID unchanged; terminal override and inheritance persist')
            hidden_before=client.screen(stream)
            files=new_tab('Files')
            native.wait_for(lambda:client.js("return !!document.querySelector('.file-list button')"),'Files')
            client.click("//section//button[contains(.,'sample.js')]",'xpath')
            native.wait_for(lambda:client.js("return !!document.querySelector('.cm-content')"),'editor')
            client.click('.cm-content')
            client.keys('draft')
            draft=client.js("return document.querySelector('.cm-content').innerText")
            assert 'draft' in draft
            appearance()
            for theme in ['original','dark-plus','light-plus','darcula','one-dark','monokai']:
                choose(0,theme)
                assert client.js("return document.querySelector('.cm-content').innerText")==draft
            button('Done')
            client.click('.editor-toolbar button[popovertarget]')
            client.click("//div[contains(@class,'editor-menu')]//button[contains(.,'Undo')]",'xpath')
            assert client.js("return document.querySelector('.cm-content').innerText")!=draft
            client.click('.editor-toolbar button[popovertarget]')
            client.click("//div[contains(@class,'editor-menu')]//button[contains(.,'Redo')]",'xpath')
            assert client.js("return document.querySelector('.cm-content').innerText")==draft
            mark('Native Files drafts and undo/redo survive all six presets while terminal continues hidden output')
            for theme in ['light-plus','one-dark']:
                appearance();choose(0,theme);button('Done');screenshot('editor-'+theme)
                assert client.js("return getComputedStyle(document.querySelector('.cm-editor')).backgroundColor")==('rgb(255, 255, 255)' if theme=='light-plus' else 'rgb(40, 44, 52)')
            diff=new_tab('Diff')
            native.wait_for(lambda:client.js("return !!document.querySelector('.diff-code-insert')"),'Diff')
            for theme in ['light-plus','one-dark']:
                appearance();choose(0,theme);button('Done')
                colors=client.js("return ['.diff-code-insert','.diff-code-delete'].map(s=>getComputedStyle(document.querySelector(s)).backgroundColor)")
                assert colors[0]!=colors[1],colors
                screenshot('diff-'+theme)
            mark('Light+ and One Dark editor/Diff computed surfaces and addition/deletion colors checked; screenshots recorded')
            client.click(f'#tab-{stream}')
            native.wait_for(lambda:client.screen(stream)!=hidden_before,'hidden stream output consumed')
            assert client.state(stream)['sessionId']==original_runtime
            assert Path(f'/proc/{shell_pid}').exists()
            appearance();choose(0,'light-plus');choose(1,'monokai');button('Done')
            second=new_tab('Stream')
            native.wait_for(lambda:'THEME_PID=' in client.screen(second),'new terminal')
            assert client.js("return document.querySelectorAll('canvas').length>=2")
            mark('Hidden terminal consumes output and reveals with same PID; new terminal opens after palette change')
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
            native.wait_for(lambda:not any(p['name']=='ongleet-gui' for p in native.owned_processes(directory)),'quit')
            client.request('DELETE','');client.session=None
            start()
            assert client.js("return document.documentElement.style.colorScheme==='light' && JSON.parse(localStorage.getItem('ongleet.appearance')).terminal==='monokai'")
            client.click(f'#tab-{files}')
            native.wait_for(lambda:client.js("return document.querySelector('.cm-content')?.innerText.includes('draft')"),'restart editor draft')
            appearance();button('Restore defaults')
            assert client.js("const p=JSON.parse(localStorage.getItem('ongleet.appearance'));return p.application==='original' && p.terminal===null")
            button('Done')
            mark('Fresh app restart restores application/terminal preferences and unsaved draft; Restore defaults resets both')
            if args.surfaces:
                # Dummy launch token + validation HTTP interception avoid Secret Service
                # access and live GitHub calls while rendering a populated PR panel.
                client.js("window.__prFixture.enable()")
                pr=new_tab('Pull request')
                native.wait_for(lambda:client.js("return !!document.querySelector('.pr-association-fields input')"),'PR association form')
                client.fill('.pr-association-fields input[placeholder="owner/repository or GitHub PR URL"]','example/repo')
                client.fill('.pr-association-fields input[placeholder="PR number"]','42')
                client.click("//section[contains(@class,'pr-panel')]//button[normalize-space()='Save']",'xpath')
                native.wait_for(lambda:client.js("return document.querySelector('.pr-heading')?.textContent.includes('Plugin native fixture')"),'fixture PR loaded')
                archive=new_tab('Archive fixture')
                native.wait_for(lambda:client.js("return document.getElementById('tab-'+arguments[0])?.textContent.includes('(0)')",archive),'archive fixture exited')
                client.click(f'#tab-{archive} + .tab-close')
                native.wait_for(lambda:len(client.invoke('archives')['entries'])==1,'fixture archive')
                client.click(f'#tab-{pr}')
                def app_menu(label):
                    client.js("const menu=document.querySelector('.window-titlebar .titlebar-menu');menu.querySelector('summary').focus();menu.open=true")
                    client.click(f"//header//details[@open]//button[normalize-space()='{label}']",'xpath')
                def themed_dialog(theme):
                    return client.js("const el=document.querySelector('dialog[open]');const s=getComputedStyle(el);return {background:s.backgroundColor,text:s.color}") == {
                        'background':'rgb(243, 243, 243)' if theme=='light-plus' else 'rgb(33, 37, 43)',
                        'text':'rgb(32, 32, 32)' if theme=='light-plus' else 'rgb(171, 178, 191)'}
                for theme in ['light-plus','one-dark']:
                    appearance();choose(0,theme);button('Done')
                    app_menu('Configure…')
                    native.wait_for(lambda:client.js("return !!document.querySelector('dialog[aria-label=\"Default actions\"] input:not(:disabled)')"),'default actions loaded')
                    assert themed_dialog(theme)
                    client.click('dialog[open] [aria-label="Action name"]')
                    focus=client.js("const el=document.activeElement;window.previousFocus=el;return {border:getComputedStyle(el).borderTopColor,focused:el.matches(':focus'),documentFocused:document.hasFocus(),accent:getComputedStyle(el).getPropertyValue('--accent')}")
                    report.setdefault('focusEvidence',[]).append({'theme':theme,**focus})
                    if focus['documentFocused']:
                        assert focus['border']==('rgb(0, 95, 184)' if theme=='light-plus' else 'rgb(97, 175, 239)'),focus
                        client.keys('\ue004')
                        assert client.js("return document.activeElement!==window.previousFocus && document.querySelector('dialog[open]').contains(document.activeElement)")
                    else:
                        report.setdefault('unavailable',[]).append(f'{theme}: headless compositor did not grant native keyboard focus; physical focus remains open')
                    screenshot('default-actions-controls-'+theme)
                    client.click('#settings-yaml')
                    client.fill('#project-actions-yaml','actions: [')
                    client.click("//dialog[@open]//button[normalize-space()='Save actions']",'xpath')
                    native.wait_for(lambda:client.js("return !!document.querySelector('dialog[open] [role=alert]')"),'invalid YAML error')
                    error_color=client.js("return getComputedStyle(document.querySelector('dialog[open] [role=alert]')).backgroundColor")
                    assert error_color==('rgb(255, 225, 228)' if theme=='light-plus' else 'rgb(86, 43, 50)'),error_color
                    screenshot('configuration-error-'+theme)
                    client.click("//dialog[@open]//button[normalize-space()='Close']",'xpath')
                    app_menu('New project…')
                    assert themed_dialog(theme)
                    screenshot('new-project-'+theme)
                    client.click("//dialog[@open]//button[normalize-space()='Cancel']",'xpath')
                    app_menu('Archives')
                    native.wait_for(lambda:client.js("return !!document.querySelector('.archives-table tbody tr')"),'archive row visible')
                    assert themed_dialog(theme)
                    screenshot('archives-'+theme)
                    client.click("//dialog[@open]//button[normalize-space()='Close']",'xpath')
                    client.js("const menu=document.querySelector('.session-pane:not([hidden]) .session-actions-menu');menu.querySelector('summary').focus();menu.open=true")
                    client.click("//details[@open]//button[normalize-space()='Configure…']",'xpath')
                    native.wait_for(lambda:client.js("return document.querySelector('dialog[open]')?.getAttribute('aria-label')==='Pull request configuration'"),'PR configuration modal')
                    assert themed_dialog(theme)
                    native.wait_for(lambda:client.js("return document.querySelector('dialog[open]')?.textContent.includes('Connected as fixture-user')"),'dummy environment account')
                    client.fill('dialog[open] .pr-association-fields input[placeholder="owner/repository or GitHub PR URL"]','invalid-repository')
                    client.click("//dialog[@open]//button[normalize-space()='Save']",'xpath')
                    native.wait_for(lambda:client.js("return !!document.querySelector('dialog[open] [role=alert]')"),'PR local validation error')
                    screenshot('pr-configuration-error-'+theme)
                    client.click("//dialog[@open]//button[normalize-space()='Cancel']",'xpath')
                    assert client.invoke('sessions')['sessions'][0]['pullRequest']=={'repository':'example/repo','number':42}
                    client.js("window.__prFixture.mode('offline')")
                    client.click('.pr-refresh')
                    native.wait_for(lambda:client.js("return document.querySelector('.pr-content')?.textContent.includes('Fixture network offline')"),'PR fixture network error')
                    screenshot('pr-network-error-'+theme)
                    client.js("window.__prFixture.mode('ok')")
                    client.click('.pr-refresh')
                    native.wait_for(lambda:client.js("return !document.querySelector('.pr-content > [role=alert]') && document.querySelector('.pr-heading')?.textContent.includes('Plugin native fixture')"),'PR restored')
                    if not client.js("return document.querySelector('.pr-checks').open"):
                        client.click('.pr-checks summary')
                    screenshot('pr-checks-'+theme)
                    mark(f'{theme}: configuration/project/archive/PR surfaces and validation/network errors checked; native focus result recorded')
                assert client.js('return window.__prFixture.calls()>0')
                assert not (directory/'config/ongleet/actions.yaml').read_text().strip().endswith('actions: [')

            report['result']='blocked' if report.get('unavailable') else 'passed'
        except Exception as error:
            report['result']='failed'
            report['error']=str(error)
            if client and client.session:
                try:(directory/'failure.txt').write_text(client.js('return document.body.innerText'))
                except Exception:pass
            raise
        finally:
            if client and client.session:
                try:
                    client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                    client.request('DELETE','')
                except Exception:pass
            try:native.wait_for(lambda:not [p for p in native.owned_processes(directory) if not p['name'].startswith('weston') and p['name'] not in ['WebKitWebDriver','Xwayland','Xvfb']],'app cleanup',timeout=10)
            except AssertionError:pass
            report['cleanupSurvivors']=[p for p in native.owned_processes(directory) if not p['name'].startswith('weston') and p['name'] not in ['WebKitWebDriver','Xwayland','Xvfb']]
            for process in report['cleanupSurvivors']:
                try:os.kill(process['pid'],signal.SIGKILL)
                except ProcessLookupError:pass
            for process in [driver,compositor]:
                if process:
                    process.terminate()
                    process.wait(timeout=5)
            (directory/'report.json').write_text(json.dumps(report,indent=2))
            print(directory,flush=True)
    return 2 if report.get('unavailable') else 0


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application',default=str(native.ROOT/'src-tauri/target/debug/ongleet-gui'))
    parser.add_argument('--surfaces',action='store_true',help='Inspect configuration/archive/PR/error surfaces with dummy environment authentication and fixture HTTP')
    parser.add_argument('--backend',choices=['wayland','xwayland','xvfb'],default='wayland')
    raise SystemExit(run(parser.parse_args()))
