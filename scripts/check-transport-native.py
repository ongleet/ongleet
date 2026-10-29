"""Reject invalid, cross-tab and stale runtime IDs through the real native SDK.

Run with with-private-display.py and a bundled native-validation executable.
"""
import argparse
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
    directory = Path(tempfile.mkdtemp(prefix='ongleet-transport-native-'))
    received = directory/'received.bin'
    probe = directory/'raw.py'
    probe.write_text('import os,pathlib,tty\ntty.setraw(0)\np=pathlib.Path('+repr(str(received))+')\np.write_bytes(b"")\nprint("TRANSPORT_READY="+str(os.getpid()),flush=True)\nwith p.open("ab",buffering=0) as f:\n while True:\n  b=os.read(0,8192)\n  if not b: break\n  f.write(b)\n')
    config=directory/'config/ongleet/projects/1/actions.yaml'
    config.parent.mkdir(parents=True)
    config.write_text(json.dumps({'actions':[
        {'id':'raw','name':'Raw','kind':'terminal','config':{'executable':'/usr/bin/python3','args':[str(probe)]}},
        {'id':'sibling','name':'Sibling','kind':'terminal','config':{'executable':'/bin/sleep','args':['600']}},
        {'id':'exit','name':'Exit 23','kind':'terminal','config':{'executable':'/usr/bin/python3','args':['-c','raise SystemExit(23)']}}
    ]}))
    env=dict(os.environ,XDG_DATA_HOME=str(directory/'data'),XDG_CONFIG_HOME=str(directory/'config'),XDG_CACHE_HOME=str(directory/'cache'),TAURI_WEBVIEW_AUTOMATION='true',TERMINAL_VALIDATION_RUN=str(directory))
    with socket.socket() as listener:
        listener.bind(('127.0.0.1',0))
        port=listener.getsockname()[1]
    report={'backend':env.get('GDK_BACKEND'),'artifacts':str(directory),'checks':[]}
    client=native.WebDriver(f'http://127.0.0.1:{port}')
    driver=None
    def mark(text):
        report['checks'].append(text)
        print('PASS:',text,flush=True)
    def cli(*arguments):
        result=subprocess.run([str(Path(args.cli).resolve()),'--json',*map(str,arguments)],env=env,capture_output=True,text=True,timeout=10)
        assert result.returncode==0,(result.stdout,result.stderr)
        return json.loads(result.stdout)
    def reject_cli_input(tab):
        before=received.read_bytes()
        result=subprocess.run([str(Path(args.cli).resolve()),'--json','tab','input',str(tab),'--text','MUST_NOT_ARRIVE'],env=env,capture_output=True,text=True,timeout=10)
        assert result.returncode!=0 and json.loads(result.stderr)['error']['code']=='unavailable',(result.stdout,result.stderr)
        assert received.read_bytes()==before
    def listing():
        return cli('tab','list','--session',client.invoke('sessions')['sessions'][0]['id'])['data']['tabs']
    def sdk(tab,method,*values):
        return client.request('POST','/execute/async',{'script':"const done=arguments[arguments.length-1]; Promise.resolve().then(()=>window.__sdkValidation.call(arguments[0],'pty',arguments[1],arguments[2])).then(value=>done({value}),error=>done({error:{code:error.code,message:String(error)}}));",'args':[tab,method,list(values)]})
    def reject_all(tab,runtime):
        for method,values in [('input',[[88]]),('resize',[90,29]),('acknowledge',[1]),('status',[]),('saveHistory',['MUST_NOT_REPLACE_HISTORY']),('close',[])]:
            result=sdk(tab,method,runtime,*values)
            assert 'error' in result and 'unavailable tab/runtime' in result['error']['message'],(method,result)
    with (directory/'native.log').open('w') as log:
        try:
            driver=subprocess.Popen([args.driver,'--host=127.0.0.1',f'--port={port}'],env=env,stdout=log,stderr=log)
            native.wait_for(lambda:client.request('GET','/status',session=False),'WebDriver')
            session=client.request('POST','/session',{'capabilities':{'alwaysMatch':{'webkitgtk:browserOptions':{'binary':str(Path(args.application).resolve())}}}},session=False)
            client.session=session['sessionId']
            native.wait_for(lambda:client.find('[aria-label="Create session"]'),'application')
            client.click('[aria-label="Create session"]')
            client.fill('#session-name','Transport test')
            client.fill('#session-directory',str(directory))
            client.click("//button[normalize-space()='Save session']",'xpath')
            tab=native.wait_for(lambda:client.js("return Number(document.querySelector('[role=tab][aria-selected=true]')?.id.slice(4)) || null"),'raw tab')
            native.wait_for(lambda:'TRANSPORT_READY=' in client.screen(tab),'raw PTY ready')
            native.wait_for(lambda:client.js('return window.__sdkValidation?.ready(arguments[0])',tab),'SDK attached')
            original=client.state(tab)
            runtime=original['sessionId']
            reject_all(tab,999999999)
            assert received.read_bytes()==b''
            assert (client.state(tab)['cols'],client.state(tab)['rows'])==(original['cols'],original['rows'])
            mark('Unknown runtime rejects input, resize, ACK, status, history and close without writing or resizing')
            client.click('[aria-label="Session and tab menu"]')
            client.click("//details[@open]//button[@title='Open a new tab'][normalize-space()='Sibling']",'xpath')
            sibling=client.invoke('sessions')['sessions'][0]['selectedTabId']
            native.wait_for(lambda:(state:=client.state(sibling)) and state.get('sessionId') and not state['replaying'],'sibling PTY')
            sibling_runtime=client.state(sibling)['sessionId']
            reject_all(tab,sibling_runtime)
            reject_all(sibling,runtime)
            assert sdk(tab,'status',runtime)['value']['state']=='running'
            assert sdk(sibling,'status',sibling_runtime)['value']['state']=='running'
            assert received.read_bytes()==b''
            mark('Cross-tab runtime IDs reject all six operations in both directions while both PTYs stay running')
            assert 'error' not in sdk(tab,'sendText',runtime,'valid café')
            native.wait_for(lambda:received.read_bytes()=='valid café'.encode(),'valid SDK bytes')
            result=sdk(tab,'acknowledge',runtime,0)
            assert 'error' in result and 'Unexpected output acknowledgement' in result['error']['message'],result
            mark('Valid runtime writes exact Unicode bytes; duplicate/invalid ACK is rejected')
            listed=listing()
            assert all(next(row for row in listed if row['id']==value)['status']['state']=='running' for value in [tab,sibling])
            cli('tab','reorder',sibling,'--position',0)
            assert [row['id'] for row in listing()]==[sibling,tab]
            assert client.state(tab)['sessionId']==runtime and client.state(sibling)['sessionId']==sibling_runtime
            mark('CLI list reports both running terminals; reorder persists without replacing either runtime')
            session_id=client.invoke('sessions')['sessions'][0]['id']
            cli('tab','create','--session',session_id,'--action','exit')
            exited=native.wait_for(lambda:next((row for row in listing() if row['id'] not in [tab,sibling] and row['status'].get('state')=='exited'),None),'CLI observed exit')
            assert exited['status']['code']==23 and not exited['resumeOnStart'],exited
            reject_cli_input(exited['id'])
            assert next(row for row in listing() if row['id']==exited['id'])['status']==exited['status']
            assert client.state(tab)['sessionId']==runtime and client.state(sibling)['sessionId']==sibling_runtime
            cli('tab','delete',exited['id'],'--confirm')
            mark('CLI reports exit code 23 and rejects input to an exited tab without restarting it or touching siblings')
            assert 'error' not in sdk(tab,'close',runtime)
            native.wait_for(lambda:next(row for row in listing() if row['id']==tab)['status']['state']=='stopped','CLI stopped status')
            reject_cli_input(tab)
            assert next(row for row in listing() if row['id']==tab)['status']['state']=='stopped'
            assert client.state(sibling)['sessionId']==sibling_runtime
            mark('CLI reports stopped status and rejects stopped-tab input without an implicit launch')
            cli('tab','close',tab,'--confirm')
            native.wait_for(lambda:not client.js('return window.__sdkValidation.ready(arguments[0])',tab),'old attachment disposed')
            assert next(row for row in listing() if row['id']==tab)['status']['state']=='closed'
            reject_cli_input(tab)
            mark('Closed definition remains listed as closed and rejects input without reopening')
            cli('tab','open',tab)
            native.wait_for(lambda:(state:=client.state(tab)) and state.get('sessionId') and state['sessionId']!=runtime and not state['replaying'],'fresh runtime for stable tab ID')
            native.wait_for(lambda:'TRANSPORT_READY=' in client.screen(tab) and client.js('return window.__sdkValidation.ready(arguments[0])',tab),'fresh SDK and raw PTY')
            current=client.state(tab)['sessionId']
            reject_all(tab,runtime)
            assert received.read_bytes()==b''
            assert sdk(tab,'status',current)['value']['state']=='running'
            assert client.state(sibling)['sessionId']==sibling_runtime
            assert sdk(sibling,'status',sibling_runtime)['value']['state']=='running'
            assert 'error' not in sdk(tab,'sendText',current,'fresh runtime only')
            native.wait_for(lambda:received.read_bytes()==b'fresh runtime only','fresh runtime valid write')
            mark('Reopened stable tab rejects its stale runtime for all operations; new runtime works and sibling remains unchanged')
        except Exception as error:
            report['error']=str(error)
            if client.session:
                try:(directory/'failure.txt').write_text(client.js('return document.body.innerText'))
                except Exception:pass
            raise
        finally:
            if client.session:
                try:
                    client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
                    client.request('DELETE','')
                except Exception:pass
            if driver:
                driver.terminate()
                driver.wait(timeout=5)
            try:native.wait_for(lambda:not native.owned_processes(directory),'test-owned process cleanup',timeout=5)
            except AssertionError:pass
            report['cleanupSurvivors']=native.owned_processes(directory)
            for process in report['cleanupSurvivors']:
                try:os.kill(process['pid'],signal.SIGKILL)
                except ProcessLookupError:pass
            (directory/'report.json').write_text(json.dumps(report,indent=2))
            print(directory,flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application',default=str(native.ROOT/'src-tauri/target/debug/ongleet-gui'))
    parser.add_argument('--cli',default=str(native.ROOT/'src-tauri/target/debug/ongleet'))
    parser.add_argument('--driver',default='/usr/bin/WebKitWebDriver')
    run(parser.parse_args())
