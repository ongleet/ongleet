"""Mixed-plugin checks used only by the isolated lifecycle harness."""
import json
import os
import subprocess
import time
from pathlib import Path


def require_private_secret_service():
    if '/tmp/' not in os.environ.get('DBUS_SESSION_BUS_ADDRESS', ''):
        raise RuntimeError('PR fixtures require a private dbus-run-session with an unlocked temporary Secret Service; refusing to touch desktop credentials')


def prepare(directory, env):
    require_private_secret_service()
    handler = directory / 'open-plugin-file.py'
    handler.write_text("import pathlib,sys\npathlib.Path(" + repr(str(directory / 'opened-file')) + ").write_text(sys.argv[1])\n")
    applications = directory / 'data/applications'
    applications.mkdir(parents=True)
    (applications / 'ongleet-file-validation.desktop').write_text('[Desktop Entry]\nType=Application\nName=Ongleet file validation\nExec=/usr/bin/python3 ' + str(handler) + ' %f\nMimeType=text/plain;\n')
    (directory / 'config').mkdir(parents=True, exist_ok=True)
    (directory / 'config/mimeapps.list').write_text('[Default Applications]\ntext/plain=ongleet-file-validation.desktop\n')
    assert 'ongleet-file-validation.desktop' in subprocess.run(['gio','mime','text/plain'], env=env, capture_output=True, text=True, check=True).stdout


def sdk(client, tab, service, method, *args):
    result = client.request('POST', '/execute/async', {
        'script': "const done=arguments[arguments.length-1]; Promise.resolve().then(()=>window.__sdkValidation.call(...arguments[0])).then(value=>done({value}),error=>done({error:{code:error.code,message:String(error)}}));",
        'args': [[tab, service, method, list(args)]],
    })
    if 'error' in result:
        raise RuntimeError(json.dumps(result['error']))
    return result.get('value')


def run(client, directory, session, terminal, stream_pid, wait_for, pid_alive, mark, cli=None):
    folder = directory / 'browser-fixture'
    folder.mkdir()
    (folder / 'unchanged.txt').write_text('read-only fixture')
    (directory / 'browser-link').symlink_to(folder, target_is_directory=True)
    path = directory / 'config/ongleet/projects/1/actions.yaml'
    settings = json.loads(path.read_text())
    settings['actions'] += [
        {'id':'sdk','name':'SDK example','kind':'sdk-example','config':{}},
        {'id':'files','name':'Files','kind':'files','config':{}},
        {'id':'pr','name':'Pull request','kind':'pull-request','config':{}},
    ]
    path.write_text(json.dumps(settings))
    if cli:
        result = cli('tab','create','--session',session['id'],'--action','files')
    else:
        result = client.invoke('create_tab', {'sessionId':session['id'],'actionId':'files'})
    files = next(s for s in result['sessions'] if s['id'] == session['id'])['selectedTabId']
    client.request('POST', '/refresh', {})
    wait_for(lambda: client.js("return !!document.querySelector(arguments[0]+' .file-list button')", f'#tab-panel-{files}'), 'file browser listing')
    # Navigate through a symlink using the real WebKitGTK button.
    client.click(f"//section[@id='tab-panel-{files}']//button[contains(.,'browser-link')]", 'xpath')
    wait_for(lambda: client.js("return document.querySelector(arguments[0]).value===arguments[1]", f'#tab-panel-{files} input[aria-label="Folder"]', str(folder)), 'symlink folder navigation')
    try:
        sdk(client, files, 'fs', 'list', {'folder':str(folder / 'missing'),'offset':0,'order':'nameAscending','showHidden':False,'query':''})
    except RuntimeError as error:
        assert 'Cannot browse' in str(error)
    else:
        raise AssertionError('Missing folder accepted')
    assert not (folder / 'missing').exists()
    client.click(f"//section[@id='tab-panel-{files}']//button[contains(.,'unchanged.txt')]", 'xpath')
    wait_for(lambda: client.js("return document.querySelector('.cm-content')?.innerText.includes('read-only fixture')"), 'inline text editor')
    assert not (directory / 'opened-file').exists()
    client.click('.editor-toolbar button[popovertarget]')
    client.click("//div[contains(@class,'editor-menu')]//button[normalize-space()='← Files']", 'xpath')
    # The native external opener remains available for non-text files.
    sdk(client, files, 'desktop', 'openFile', str(folder / 'unchanged.txt'))
    wait_for(lambda: (directory / 'opened-file').exists(), 'associated file application')
    assert (directory / 'opened-file').read_text() == str(folder / 'unchanged.txt')
    assert (folder / 'unchanged.txt').read_text() == 'read-only fixture'
    assert client.state(terminal)['sessionId'] and pid_alive(stream_pid)
    mark('Files plugin follows directory symlinks, opens files through a private associated application, retains navigation, rejects missing paths and preserves sibling PTY')
    sdk_contract_checks(client, directory, session['id'], wait_for, pid_alive, mark)
    result = client.invoke('create_tab', {'sessionId':session['id'],'actionId':'pr'})
    pr = next(s for s in result['sessions'] if s['id'] == session['id'])['selectedTabId']
    wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])', pr), 'PR SDK attachment')
    pr_auth_checks(client, session['id'], pr, terminal, wait_for, mark)
    client.invoke('close_tab', {'id':pr,'confirmed':False})
    return files, pr, str(folder)


def restored(client, session_id, saved, mark):
    files, pr, folder = saved
    session = next(s for s in client.invoke('sessions')['sessions'] if s['id'] == session_id)
    assert session['pullRequest'] == {'repository':'example/repo','number':42}
    assert next(t for t in session['tabs'] if t['id'] == files)['config']['folder'] == folder
    assert next(t for t in session['tabs'] if t['id'] == files)['isOpen']
    assert not next(t for t in session['tabs'] if t['id'] == pr)['isOpen']
    mark('Mixed plugin openings, file folder and session PR association survive Quit/restart independently of terminal restart flags')


def before_quit(client, directory, session_id, wait_for):
    client.js("window.__prFixture.enable('wait')")
    client.invoke('create_tab', {'sessionId': session_id, 'actionId': 'pr'})
    client.request('POST','/refresh',{})
    wait_for(lambda: client.js('return window.__prFixture.active()>0'), 'pending fixture request before Quit')
    return None


def after_quit(directory, pending_pid, pid_alive, mark):
    mark('Quit completed with an outstanding frontend HTTP fixture; native transport cancellation has separate Rust coverage')


def pr_auth_checks(client, session_id, pr, terminal, wait_for, mark):
    # Run only on an isolated Secret Service bus with fixture credentials.
    require_private_secret_service()
    client.js("window.__prFixture.enable()")
    current = sdk(client, pr, 'storage', 'read', 'session')
    sdk(client, pr, 'storage', 'replace', 'session', current['revision'] if current else 0, 1, {'repository':'example/repo','number':42})
    def text(tab): return client.js('return document.querySelector(arguments[0])?.innerText', f'#tab-panel-{tab}') or ''
    def button(tab, label): client.click(f"//section[@id='tab-panel-{tab}']//button[normalize-space()='{label}']", 'xpath')
    def connect(tab, token):
        client.fill(f'#tab-panel-{tab} input[type=password]', token)
        button(tab, 'Replace token' if 'Connected as' in text(tab) else 'Connect')
    connect(pr, 'first-fixture')
    wait_for(lambda: 'Plugin native fixture' in text(pr), 'connected PR data')
    other = client.invoke('create_tab', {'sessionId':session_id,'actionId':'pr'})['sessions'][0]['selectedTabId']
    client.request('POST','/refresh',{})
    wait_for(lambda: 'Connected as fixture-user' in text(other), 'shared account in second tab')
    client.click(f'#tab-{pr}')
    button(pr, 'Replace token')
    connect(pr, 'invalid-fixture')
    wait_for(lambda: 'invalid' in text(pr), 'invalid replacement error')
    assert 'Connected as fixture-user' in text(other)
    assert 'Plugin native fixture' in text(pr)
    connect(pr, 'second-fixture')
    wait_for(lambda: 'Connected as second-user' in text(other), 'replacement shared across tabs')
    client.js("window.__prFixture.mode('offline')")
    client.click(f'#tab-panel-{pr} .pr-refresh')
    wait_for(lambda: 'offline' in text(pr), 'network error')
    assert 'Plugin native fixture' in text(pr)
    client.js("window.__prFixture.mode('ok')")
    client.request('POST', '/refresh', {})
    wait_for(lambda: 'Connected as second-user' in text(pr), 'Secret Service connection restored after reload')
    wait_for(lambda: 'Plugin native fixture' in text(other), 'second attachment refreshed')
    client.click(f'#tab-{terminal}')
    previous = client.js('return window.__prFixture.calls()')
    wait_for(lambda: client.js('return window.__prFixture.calls()>arguments[0]', previous), 'real hidden polling timer', timeout=130)
    mark('WebKitGTK shares token authentication, preserves connection after invalid replacement, restores from native credentials and polls hidden tabs')
    client.click(f'#tab-{pr}')
    client.js("window.__prFixture.mode('wait')")
    wait_for(lambda: not client.js('return document.querySelector(arguments[0]).disabled', f'#tab-panel-{pr} .pr-refresh'), 'refresh idle')
    client.click(f'#tab-panel-{pr} .pr-refresh')
    wait_for(lambda: client.js('return window.__prFixture.active()>0'), 'outstanding request')
    button(pr, 'Disconnect')
    wait_for(lambda: 'Connected as' not in text(other), 'disconnect synchronized')
    wait_for(lambda: client.js('return window.__prFixture.active()===0'), 'request abort')
    assert 'Plugin native fixture' not in text(pr) and 'Plugin native fixture' not in text(other)
    assert sdk(client, pr, 'credentials', 'get', 'github-account') is None
    client.invoke('close_tab', {'id':other,'confirmed':False})
    client.request('POST','/refresh',{})
    wait_for(lambda: client.js('return !!window.__prFixture'), 'fixture bridge after reload')
    wait_for(lambda: client.js('return !!document.querySelector(arguments[0])', f'#tab-panel-{pr} input[type=password]:not(:disabled)'), 'reconnect field')
    client.js("window.__prFixture.mode('ok')")
    connect(pr, 'first-fixture')
    wait_for(lambda: 'Plugin native fixture' in text(pr), 'reconnected fixture')
    mark('Disconnect deletes native credential, aborts pending request and clears PR data across WebKitGTK tabs')


def sdk_contract_checks(client, directory, session_id, wait_for, pid_alive, mark):
    result = client.invoke('create_tab', {'sessionId':session_id,'actionId':'sdk'})
    tab = next(s for s in result['sessions'] if s['id'] == session_id)['selectedTabId']
    client.request('POST','/refresh',{})
    wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])', tab), 'example SDK attachment')
    client.click(f'#tab-panel-{tab} button')
    wait_for(lambda: client.js("return document.querySelector(arguments[0])?.innerText.includes('saved run 1')", f'#tab-panel-{tab}'), 'frontend-only fs/process/storage example')
    current = sdk(client, tab, 'storage', 'read', 'tab')
    assert current['value'] == {'runs':1}
    try: sdk(client, tab, 'storage', 'replace', 'tab', 0, 1, {'runs':999})
    except RuntimeError as error: assert 'revision_conflict' in str(error)
    else: raise AssertionError('Stale namespace update accepted')
    assert sdk(client, tab, 'storage', 'read', 'tab')['value'] == {'runs':1}
    for options, expected in [
        ({'executable':'/usr/bin/python3','args':['-c','import time; time.sleep(60)'],'timeoutMs':80}, 'timeout'),
        ({'executable':'/usr/bin/python3','args':['-c',"import sys; sys.stderr.write('x'*600000)"]}, 'limit_exceeded'),
    ]:
        try: sdk(client, tab, 'process', 'run', options)
        except RuntimeError as error: assert expected in str(error), str(error)
        else: raise AssertionError('Process bound ignored')
    client.request('POST', '/execute/async', {
        'script': "const done=arguments[arguments.length-1]; window.__sdkEvents=0; window.__sdkValidation.call(arguments[0], 'storage', 'subscribe', [()=>window.__sdkEvents++]).then(unsubscribe=>{window.__sdkUnsubscribe=unsubscribe;done(true)},error=>done(String(error)));",
        'args':[tab],
    })
    sdk(client, tab, 'storage', 'configure', {})
    wait_for(lambda: client.js('return window.__sdkEvents>0'), 'SDK change notification')
    client.js('window.__sdkUnsubscribe(); window.__sdkEvents=0; return true')
    sdk(client, tab, 'storage', 'configure', {})
    assert client.request('POST', '/execute/async', {'script':'const done=arguments[arguments.length-1]; setTimeout(()=>done(window.__sdkEvents),200)', 'args':[]}) == 0, 'Disposed subscription still receives changes'
    pid_file = directory / 'sdk-child'
    options = {'executable':'/usr/bin/python3','args':['-c', 'import os,pathlib,time; pathlib.Path('+repr(str(pid_file))+').write_text(str(os.getpid())); time.sleep(60)']}
    def launch():
        pid_file.unlink(missing_ok=True)
        client.js("window.__sdkJob = window.__sdkValidation.call(arguments[0], 'process', 'run', [arguments[1]]).then(value=>({value}),error=>({code:error.code,message:String(error)})); return true", tab, options)
        wait_for(lambda: pid_file.exists() and pid_file.read_text(), 'SDK child started')
        return int(pid_file.read_text())
    pid = launch()
    try: sdk(client, tab, 'process', 'run', {'executable':'/usr/bin/true'})
    except RuntimeError as error: assert 'already running' in str(error)
    else: raise AssertionError('Overlapping native process accepted')
    sdk(client, tab, 'process', 'cancel')
    assert not pid_alive(pid), 'Cancellation returned before child cleanup'
    result = client.request('POST', '/execute/async', {'script':'const done=arguments[arguments.length-1]; window.__sdkJob.then(done)', 'args':[]})
    assert result['code'] == 'cancelled', result
    pid = launch()
    client.request('POST','/refresh',{})
    wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])', tab), 'SDK reload reattached')
    wait_for(lambda: not pid_alive(pid), 'reload cancels old attachment process')
    assert sdk(client, tab, 'storage', 'read', 'tab')['value'] == {'runs':1}
    pid = launch()
    client.invoke('archive_tab', {'id':tab,'confirmed':False})
    wait_for(lambda: not pid_alive(pid), 'archive joins SDK child')
    archive = client.invoke('archives')['entries'][0]['id']
    client.request('POST','/refresh',{})
    client.invoke('restore_archive', {'id':archive})
    client.request('POST','/refresh',{})
    wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])', tab), 'restored SDK attachment')
    assert sdk(client, tab, 'storage', 'read', 'tab')['value'] == {'runs':1}
    client.invoke('close_tab', {'id':tab,'confirmed':False})
    mark('Frontend-only SDK example crosses fs/process/storage boundary; structured conflicts/timeouts/flood errors, overlap rejection, cancellation, reload cleanup and archive restoration pass')
