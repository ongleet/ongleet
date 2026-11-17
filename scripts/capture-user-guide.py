"""Capture user-guide screenshots in an isolated native Linux workspace."""
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
    directory = Path(tempfile.mkdtemp(prefix='ongleet-user-guide-'))
    folder = directory / 'repository'
    folder.mkdir()
    source = '// Checkout confirmation shown after a successful payment.\nexport function confirmation(order) {\n  const total = formatTotal(order.total, order.currency);\n  return `Payment: ${total}`;\n}\n\nexport function formatTotal(amount, currency = "EUR") {\n  return new Intl.NumberFormat("en-GB", {\n    style: "currency",\n    currency,\n  }).format(amount);\n}\n\nexport function checkoutSummary(order) {\n  return {\n    reference: order.reference,\n    message: confirmation(order),\n    items: order.items.length,\n  };\n}\n'
    (folder/'checkout.js').write_text(source)
    (folder/'README.md').write_text('# Storefront\n\nCheckout confirmation demo.\n')
    (folder/'tests').mkdir()
    (folder/'tests/checkout.test.js').write_text('import { confirmation } from "../checkout.js";\n\n// Verify the confirmation includes the order reference.\n')
    def git(*argv):
        subprocess.run(['git', '-C', str(folder), *argv], check=True, capture_output=True)
    git('init','-b','main')
    git('config','user.email','fixture@example.test')
    git('config','user.name','Fixture')
    git('add','.')
    git('commit','-m','Fixture baseline')
    (folder/'checkout.js').write_text(source.replace('return `Payment: ${total}`;', 'return `Payment confirmed: ${total} — order ${order.reference}`;'))
    git('add','.')
    git('commit','-m','Fixture change')
    actions = directory/'config/ongleet/projects/1/actions.yaml'
    actions.parent.mkdir(parents=True)
    actions.write_text(json.dumps({'actions':[
        {'id':'stream','name':'Shell','kind':'terminal','config':{'executable':'/bin/bash','args':['--noprofile','--norc']}},
        {'id':'files','name':'Files','kind':'files','config':{}},
        {'id':'diff','name':'Diff','kind':'diff','config':{'mode':'refs','baseRef':'HEAD~1','targetRef':'HEAD'}},
        {'id':'note','name':'Notes','kind':'note','config':{}},
        {'id':'search','name':'Search','kind':'search','config':{}},
        {'id':'pr','name':'Pull request','kind':'pull-request','config':{}},
    ]}))
    runtime = directory/'runtime'
    runtime.mkdir(mode=0o700)
    env = dict(os.environ, XDG_DATA_HOME=str(directory/'data'), XDG_CONFIG_HOME=str(directory/'config'), XDG_CACHE_HOME=str(directory/'cache'), XDG_RUNTIME_DIR=str(runtime), TAURI_WEBVIEW_AUTOMATION='true', TERMINAL_VALIDATION_RUN=str(directory))
    env.pop('DISPLAY',None)
    env.pop('WAYLAND_DISPLAY',None)
    env.pop('GH_TOKEN', None)
    env.pop('GITHUB_TOKEN', None)
    report = {'backend':args.backend,'artifacts':str(directory)}
    client = None
    compositor = driver = None
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
                button('Configure…')
                native.wait_for(lambda:client.js('return !!document.querySelector("#settings-appearance:not(:disabled)")'),'appearance tab')
                client.click('#settings-appearance')
                native.wait_for(lambda:client.js('return !!document.querySelector("#appearance-title")'),'appearance settings')
            start()
            client.request('POST', '/window/rect', {'width': 1280, 'height': 900})
            client.click('[aria-label="Create session"]')
            client.fill('#session-name', 'Improve checkout')
            client.fill('#session-directory', str(folder))
            button('Save session')
            session=native.wait_for(lambda: client.invoke('sessions')['sessions'] or None,'session')[0]
            def capture(name):
                time.sleep(1)
                output=native.ROOT/'docs/images'
                output.mkdir(exist_ok=True)
                (output/(name+'.png')).write_bytes(base64.b64decode(client.request('GET','/screenshot')))
                print(name, flush=True)
            def tab(action):
                names={'files':'Files','diff':'Diff','note':'Notes','search':'Search','pr':'Pull request'}
                client.click('[aria-label="Session and tab menu"]')
                client.click(f"//details[@open]//button[@title='Open a new tab'][normalize-space()='{names[action]}']", 'xpath')
                time.sleep(1)
                return client.invoke('sessions')['sessions'][0]['selectedTabId']
            shell=session['selectedTabId']
            native.wait_for(lambda: subprocess.run([str(Path(args.application).resolve().with_name('ongleet')), 'tab','input',str(shell),'--text','git --no-pager log --oneline -2','--enter'],env=env,capture_output=True).returncode == 0, 'shell input')
            capture('terminal')
            files=tab('files')
            def command(*argv):
                result=subprocess.run([str(Path(args.application).resolve().with_name('ongleet')),'--json',*map(str,argv)],env=env,text=True,capture_output=True)
                if result.returncode:
                    raise RuntimeError(result.stderr or result.stdout)
                return json.loads(result.stdout)['data']
            command('tab','invoke',files,'open-path','--args',json.dumps({'path':str(folder/'checkout.js')}))
            capture('files')
            tab('diff')
            capture('diff')
            note=tab('note')
            native.wait_for(lambda: command('tab','capabilities',note)['stateAvailable'], 'note provider')
            state=command('tab','state',note)['state']['data']
            command('tab','invoke',note,'set-text','--args',json.dumps({'noteId':state['selectedNoteId'],'revision':state['revision'],'text':'# Checkout review\n\n## Goal\nMake the checkout confirmation clearer.\n\n## Checklist\n- [x] Inspect the current implementation\n- [ ] Review the diff\n- [ ] Run the checkout tests\n\n## Agent handoff\nKeep the existing payment API and add a regression test.\n'}))
            capture('notes')
            tab('search')
            client.fill('[aria-label="Search file contents"]', 'confirmation')
            time.sleep(2)
            native.wait_for(lambda: client.js("return !!document.querySelector('.search-matches button')"), 'search results')
            client.click('.search-matches button')
            capture('search')
            tab('pr')
            capture('pull-request')
            appearance()
            capture('appearance')
            report['result']='captured'
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
    return 0


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application',default=str(native.ROOT/'src-tauri/target/debug/ongleet-gui'))
    parser.add_argument('--backend',choices=['wayland','xwayland','xvfb'],default='wayland')
    raise SystemExit(run(parser.parse_args()))
