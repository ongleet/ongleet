"""Isolated WebKitGTK PR authentication acceptance. Run under dbus-run-session.

The caller must start a private unlocked Secret Service and private X11 display.
Only GitHub HTTP responses are fixtures; credentials use the real native SDK.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def run(args):
    if '/tmp/' not in os.environ.get('DBUS_SESSION_BUS_ADDRESS','') or not os.environ.get('XDG_DATA_HOME','').startswith('/tmp/'):
        raise RuntimeError('Refusing credential writes: use a private dbus-run-session and temporary XDG_DATA_HOME')
    native = module('native', 'check-native.py')
    plugins = module('plugins', 'plugin-native-checks.py')
    directory = Path(tempfile.mkdtemp(prefix='devcenter-pr-auth-'))
    config = directory / 'config/devcenter/projects/1'
    config.mkdir(parents=True)
    (config / 'actions.yaml').write_text(json.dumps({'actions': [
        {'id':'shell','name':'Shell','kind':'terminal','config':{'executable':'/bin/true','args':[]}},
        {'id':'pr','name':'Pull request','kind':'pull-request','config':{'refreshIntervalSeconds':30}},
    ]}))
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    env = dict(os.environ, XDG_DATA_HOME=str(directory / 'data'), XDG_CONFIG_HOME=str(directory / 'config'), XDG_CACHE_HOME=str(directory / 'cache'), TAURI_WEBVIEW_AUTOMATION='true', GDK_BACKEND='x11', TERMINAL_VALIDATION_RUN=str(directory))
    client = native.WebDriver(f'http://127.0.0.1:{port}')
    stalled = socket.socket()
    stalled.bind(('127.0.0.1',0))
    stalled.listen(16)
    stalled_connections = []
    def accept_stalled():
        while True:
            try: connection, _ = stalled.accept(); stalled_connections.append(connection)
            except OSError: return
    threading.Thread(target=accept_stalled,daemon=True).start()
    stalled_url = f'https://127.0.0.1:{stalled.getsockname()[1]}'
    checks = []
    report = {'artifacts':str(directory), 'github':'fixture responses; no live acceptance', 'credentials':'real Secret Service on caller-owned private bus', 'checks':checks}
    def mark(message):
        checks.append(message)
        print(message, flush=True)
    try:
        with (directory / 'driver.log').open('w') as log:
            driver = subprocess.Popen([args.driver, '--host=127.0.0.1', f'--port={port}'], env=env, stdout=log, stderr=log, start_new_session=True)
            native.wait_for(lambda: client.request('GET','/status',session=False), 'driver')
            def launch():
                result = client.request('POST','/session',{'capabilities':{'alwaysMatch':{'webkitgtk:browserOptions':{'binary':str(Path(args.application).resolve())}}}},session=False)
                client.session = result['sessionId']
                native.wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), 'application')
            launch()
            client.click('[aria-label="Create session"]')
            client.fill('#session-name','PR authentication')
            client.fill('#session-directory',str(directory))
            client.click("//button[normalize-space()='Save session']",'xpath')
            state = native.wait_for(lambda: (snapshot if snapshot['sessions'] else None) if (snapshot := client.invoke('sessions')) else None,'session')
            session = state['sessions'][0]
            terminal = session['selectedTabId']
            native.wait_for(lambda: client.js('return !!window.__prFixture'), 'validation fixture bridge')
            client.js('window.__prFixture.enable()')
            result = client.invoke('create_tab',{'sessionId':session['id'],'actionId':'pr'})
            pr = result['sessions'][0]['selectedTabId']
            client.request('POST','/refresh',{})
            native.wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])',pr),'PR attachment')
            for tab, service, method, options in [(terminal,'credentials','get','account'),(pr,'process','run',{'executable':'/bin/true'}),(pr,'http','request',{'url':'http://example.com','method':'GET'})]:
                try: plugins.sdk(client,tab,service,method,options)
                except RuntimeError as error: assert 'capability' in str(error) or 'HTTPS' in str(error), str(error)
                else: raise AssertionError('Native capability/HTTPS validation bypassed')
            client.js('''window.__httpResults=[]; for(let i=0;i<5;i++) window.__TAURI_INTERNALS__.invoke('sdk_request',{id:arguments[0],attachment:'invalid',request:{operation:'httpCancel',token:'probe'}}).catch(e=>window.__httpResults.push(e));''',pr)
            native.wait_for(lambda: client.js('return window.__httpResults.length===5'),'attachment ownership rejection')
            assert client.js("return window.__httpResults.every(e=>e.message.includes('attachment unavailable'))")
            mark('Native SDK rejects credentials/process capability violations, insecure HTTP and invalid attachment ownership')
            client.js('''window.__pendingHttp=[];window.__httpResults=[];
                for(let i=0;i<5;i++) {const c=new AbortController();window.__pendingHttp.push(c);
                  window.__sdkValidation.call(arguments[0],'http','request',[{url:arguments[1],method:'GET',signal:c.signal}]).then(()=>window.__httpResults.push({code:'unexpected'}),e=>window.__httpResults.push({code:e.code}));}
                ''',pr,stalled_url)
            native.wait_for(lambda: client.js("return window.__httpResults.some(r=>r.code==='limit_exceeded')"),'native HTTP concurrency bound')
            client.js('window.__pendingHttp.forEach(c=>c.abort())')
            native.wait_for(lambda: client.js('return window.__httpResults.length===5'),'native HTTP abort cleanup',timeout=5)
            assert client.js("return window.__httpResults.filter(r=>r.code==='cancelled').length===4")
            mark('Native HTTP enforces four concurrent requests per attachment and aborts stalled TLS I/O')
            extra_tabs = [client.invoke('create_tab',{'sessionId':session['id'],'actionId':'pr'})['sessions'][0]['selectedTabId'] for _ in range(8)]
            client.request('POST','/refresh',{})
            native.wait_for(lambda: client.js('return arguments[0].every(id=>window.__sdkValidation?.ready(id))',extra_tabs+[pr]),'HTTP global limit attachments')
            client.js('''window.__pendingHttp=[];window.__httpResults=[];
                for(const id of arguments[0]) for(let i=0;i<4;i++) {const c=new AbortController();window.__pendingHttp.push(c);
                  window.__sdkValidation.call(id,'http','request',[{url:arguments[1],method:'GET',signal:c.signal}]).then(()=>window.__httpResults.push({code:'unexpected'}),e=>window.__httpResults.push({code:e.code}));}
                ''',extra_tabs+[pr],stalled_url)
            native.wait_for(lambda: client.js("return window.__httpResults.filter(r=>r.code==='limit_exceeded').length===4"),'global 32-request limit')
            client.js('window.__pendingHttp.forEach(c=>c.abort())')
            native.wait_for(lambda: client.js('return window.__httpResults.length===36'),'global HTTP cancellation',timeout=5)
            assert client.js("return window.__httpResults.filter(r=>r.code==='cancelled').length===32")
            for tab in extra_tabs: client.invoke('close_tab',{'id':tab,'confirmed':False})
            client.invoke('select_tab',{'id':pr})
            client.request('POST','/refresh',{})
            native.wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])',pr),'PR attachment after saturation')
            mark('Native HTTP enforces 32 requests globally across nine attachments and cancels all stalled work')
            plugins.pr_auth_checks(client,session['id'],pr,terminal,native.wait_for,mark)
            previous_connections = len(stalled_connections)
            client.js("window.__sdkValidation.call(arguments[0],'http','request',[{url:arguments[1],method:'GET'}]).catch(()=>{})",pr,stalled_url)
            native.wait_for(lambda: len(stalled_connections)>previous_connections,'pending native HTTP before Quit')
            client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
            native.wait_for(lambda: not any(p['name'].startswith('devcenter') for p in native.owned_processes(directory)), 'Quit', timeout=10)
            try: client.request('DELETE','')
            except Exception: pass
            client.session = None
            launch()
            native.wait_for(lambda: client.js("return document.querySelector(arguments[0])?.innerText.includes('Connected as fixture-user')",f'#tab-panel-{pr}'),'account restored after native restart')
            mark('Native Quit cancels stalled TLS I/O; restart restores shared account from Secret Service')
            client.js('window.__prFixture.enable()')
            client.click(f"//section[@id='tab-panel-{pr}']//button[normalize-space()='Disconnect']",'xpath')
            native.wait_for(lambda: plugins.sdk(client,pr,'credentials','get','github-account') is None,'credential deleted after restart')
            report['passed'] = True
    except Exception as error:
        report['error'] = str(error)
        try: report['ui'] = client.js('return document.body.innerText')
        except Exception: pass
        report['passed'] = False
        raise
    finally:
        if client.session:
            try: client.js("setTimeout(()=>window.__TAURI_INTERNALS__.invoke('quit',{confirmed:true}),0)")
            except Exception: pass
            try: client.request('DELETE','')
            except Exception: pass
        if 'driver' in locals():
            driver.terminate()
            try: driver.wait(timeout=5)
            except subprocess.TimeoutExpired: driver.kill(); driver.wait()
        stalled.close()
        for connection in stalled_connections: connection.close()
        try: native.wait_for(lambda: not native.owned_processes(directory),'native cleanup',timeout=5)
        except AssertionError: pass
        report['survivors'] = native.owned_processes(directory)
        (directory / 'report.json').write_text(json.dumps(report,indent=2))
        print(directory / 'report.json',flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application',required=True)
    parser.add_argument('--driver',required=True)
    run(parser.parse_args())
