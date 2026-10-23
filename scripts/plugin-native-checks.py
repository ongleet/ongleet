"""Mixed-plugin checks used only by the isolated lifecycle harness."""
import json
import os
import subprocess
import time
from pathlib import Path


def prepare(directory, env):
    tools = directory / 'plugin-tools'
    tools.mkdir()
    gh = tools / 'gh'
    gh.write_text('''#!/usr/bin/python3
import json, os, pathlib, sys, time
root = pathlib.Path(os.environ['TERMINAL_VALIDATION_RUN'])
with (root / 'gh-calls.jsonl').open('a') as log:
    log.write(json.dumps({'pid': os.getpid(), 'args': sys.argv[1:]}) + '\\n')
mode = (root / 'gh-mode').read_text() if (root / 'gh-mode').exists() else 'ok'
if mode == 'wait': time.sleep(60)
if mode == 'error':
    print('fixture: gh auth login required', file=sys.stderr)
    sys.exit(4)
print(json.dumps({'title':'Plugin native fixture','url':'https://github.com/example/repo/pull/42','number':42,'state':'OPEN','reviewDecision':'APPROVED','statusCheckRollup':[{'name':'build','conclusion':'SUCCESS'}]}))
''')
    gh.chmod(0o700)
    env['PATH'] = str(tools) + os.pathsep + env['PATH']
    handler = directory / 'open-plugin-file.py'
    handler.write_text("import pathlib,sys\npathlib.Path(" + repr(str(directory / 'opened-file')) + ").write_text(sys.argv[1])\n")
    applications = directory / 'data/applications'
    applications.mkdir(parents=True)
    (applications / 'devcenter-file-validation.desktop').write_text('[Desktop Entry]\nType=Application\nName=DevCenter file validation\nExec=/usr/bin/python3 ' + str(handler) + ' %f\nMimeType=text/plain;\n')
    (directory / 'config').mkdir(parents=True, exist_ok=True)
    (directory / 'config/mimeapps.list').write_text('[Default Applications]\ntext/plain=devcenter-file-validation.desktop\n')
    assert 'devcenter-file-validation.desktop' in subprocess.run(['gio','mime','text/plain'], env=env, capture_output=True, text=True, check=True).stdout


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
    path = directory / 'config/devcenter/projects/1/actions.yaml'
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
    sdk(client, pr, 'storage', 'replace', 'session', 0, 1, {'repository':'example/repo','number':42})
    client.request('POST', '/refresh', {})
    wait_for(lambda: client.js("return document.querySelector(arguments[0])?.innerText.includes('Plugin native fixture')", f'#tab-panel-{pr}'), 'GitHub fixture status')
    calls = [json.loads(line) for line in (directory / 'gh-calls.jsonl').read_text().splitlines()]
    assert calls[-1]['args'][:6] == ['pr','view','42','--repo','example/repo','--json']
    # Use the actual 60-second WebKitGTK timer, with no fake clock.
    client.click(f'#tab-{terminal}')
    hidden_count = len((directory / 'gh-calls.jsonl').read_text().splitlines())
    deadline = time.monotonic() + 62
    while time.monotonic() < deadline:
        assert len((directory / 'gh-calls.jsonl').read_text().splitlines()) == hidden_count
        time.sleep(0.25)
    client.click(f'#tab-{pr}')
    wait_for(lambda: len((directory / 'gh-calls.jsonl').read_text().splitlines()) > hidden_count, 'visible PR refresh', timeout=70)
    visible_count = len((directory / 'gh-calls.jsonl').read_text().splitlines())
    wait_for(lambda: len((directory / 'gh-calls.jsonl').read_text().splitlines()) > visible_count, 'visible PR 60-second polling', timeout=70)
    wait_for(lambda: client.js("return !document.querySelector(arguments[0]).disabled", f"#tab-panel-{pr} > button"), 'periodic refresh completion')
    calls = [json.loads(line) for line in (directory / 'gh-calls.jsonl').read_text().splitlines()]
    mark('PR polling pauses while hidden and refreshes through the real 60-second WebKitGTK timer when visible')
    (directory / 'gh-mode').write_text('error')
    client.click(f"//section[@id='tab-panel-{pr}']//button[normalize-space()='Refresh']", 'xpath')
    wait_for(lambda: client.js("return document.querySelector(arguments[0])?.innerText.includes('gh auth login required')", f'#tab-panel-{pr}'), 'gh authentication error')
    assert client.js("return document.querySelector(arguments[0]).innerText.includes('Plugin native fixture') && document.querySelector(arguments[0]).innerText.includes('Last successful refresh')", f'#tab-panel-{pr}')
    (directory / 'gh-mode').write_text('wait')
    client.click(f"//section[@id='tab-panel-{pr}']//button[normalize-space()='Refresh']", 'xpath')
    wait_for(lambda: len((directory / 'gh-calls.jsonl').read_text().splitlines()) > len(calls) + 1, 'pending gh child')
    pending_pid = json.loads((directory / 'gh-calls.jsonl').read_text().splitlines()[-1])['pid']
    try:
        sdk(client, pr, 'process', 'run', {'executable':'gh','args':['pr','view','42','--repo','example/repo']})
    except RuntimeError as error:
        assert 'already running' in str(error)
    else:
        raise AssertionError('Overlapping PR refresh accepted')
    # Replacing the shared namespace must join old work before notifying tabs.
    (directory / 'gh-mode').write_text('ok')
    current = sdk(client, pr, 'storage', 'read', 'session')
    sdk(client, pr, 'storage', 'replace', 'session', current['revision'], 1, {'repository':'example/changed','number':43})
    assert not pid_alive(pending_pid), 'Association change returned before old child cleanup'
    wait_for(lambda: client.js("return !document.querySelector(arguments[0]).disabled", f"#tab-panel-{pr} > button"), 'changed association refresh')
    current = sdk(client, pr, 'storage', 'read', 'session')
    sdk(client, pr, 'storage', 'replace', 'session', current['revision'], 1, {'repository':'example/repo','number':42})
    wait_for(lambda: client.js("return !document.querySelector(arguments[0]).disabled", f"#tab-panel-{pr} > button"), 'restored association refresh')
    (directory / 'gh-mode').write_text('wait')
    previous = len((directory / 'gh-calls.jsonl').read_text().splitlines())
    client.click(f"//section[@id='tab-panel-{pr}']//button[normalize-space()='Refresh']", 'xpath')
    wait_for(lambda: len((directory / 'gh-calls.jsonl').read_text().splitlines()) > previous, 'pending child after association change')
    pending_pid = json.loads((directory / 'gh-calls.jsonl').read_text().splitlines()[-1])['pid']
    client.invoke('close_tab', {'id':pr,'confirmed':False})
    wait_for(lambda: not pid_alive(pending_pid), 'PR child cancelled on close')
    client.request('POST', '/refresh', {})
    client.invoke('select_tab', {'id':terminal})
    client.request('POST', '/refresh', {})
    wait_for(lambda: client.state(terminal) and not client.state(terminal)['replaying'], 'sibling terminal reattached')
    assert pid_alive(stream_pid)
    if cli:
        tabs = cli('tab','list','--session',session['id'])['tabs']
        assert next(t for t in tabs if t['id'] == files)['kind'] == 'files'
        assert not next(t for t in tabs if t['id'] == pr)['isOpen']
    mark('PR plugin shows checks/reviews, retains data on authentication failure, rejects overlap and kills pending gh on close')
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
    (directory / 'gh-mode').write_text('wait')
    previous = len((directory / 'gh-calls.jsonl').read_text().splitlines())
    client.invoke('create_tab', {'sessionId': session_id, 'actionId': 'pr'})
    client.request('POST', '/refresh', {})
    wait_for(lambda: len((directory / 'gh-calls.jsonl').read_text().splitlines()) > previous, 'pending PR request before Quit')
    return json.loads((directory / 'gh-calls.jsonl').read_text().splitlines()[-1])['pid']


def after_quit(directory, pending_pid, pid_alive, mark):
    assert not pid_alive(pending_pid), 'Quit left a live gh subprocess'
    (directory / 'gh-mode').write_text('ok')
    mark('Quit waits for pending PR subprocess cleanup before exiting')


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
