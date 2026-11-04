"""Acceptance checks called by check-diff-native.py --interactions-only."""
import json
import subprocess


def check(client, native, directory, env, session, diff, mark, cli):
    def command(*args, error=None):
        result = subprocess.run([str(cli), '--json', *map(str, args)], env=env, text=True, capture_output=True, timeout=12)
        response = json.loads(result.stdout or result.stderr)
        if error:
            assert not response['ok'] and response['error']['code'] == error, response
            return response['error']
        if not response['ok']: raise RuntimeError(response)
        return response['data']
    def sdk(caller, method, *args, error=None):
        result = client.request('POST', '/execute/async', {
            'script': "const done=arguments[arguments.length-1];window.__sdkValidation.call(arguments[0],'tabs',arguments[1],arguments[2]).then(value=>done({value}),error=>done({error:{code:error.code,message:String(error)}}));",
            'args': [caller, method, list(args)],
        })
        if error:
            assert result.get('error', {}).get('code') == error, result
            return result['error']
        assert 'error' not in result, result
        return result['value']
    def state(tab, args={}): return command('tab', 'state', tab, '--args', json.dumps(args))['state']
    def invoke(tab, action, args, **kwargs): return command('tab', 'invoke', tab, action, '--args', json.dumps(args), **kwargs)
    native.wait_for(lambda: command('tab', 'capabilities', diff)['stateAvailable'], 'Diff provider')
    diff_state = state(diff)
    assert diff_state['data']['totalFiles'] == 2, diff_state
    page = state(diff, {'limit': 1})['data']
    assert len(page['files']) == 1 and page['nextOffset'] == 1
    path = page['files'][0]['path']
    assert invoke(diff, 'set-files-expanded', {'paths': [path], 'expanded': False}) == {'updated': 1}
    native.wait_for(lambda: client.js("return document.querySelector('.diff-fold').getAttribute('aria-expanded')==='false'"), 'CLI collapse updates UI')
    invoke(diff, 'set-files-expanded', {'paths': [path, 'missing'], 'expanded': True}, error='invalid_arguments')
    assert state(diff)['data']['files'][0]['expanded'] is False
    client.click('.diff-fold')
    native.wait_for(lambda: state(diff)['data']['files'][0]['expanded'], 'UI expansion updates state')
    revision = diff_state['data']['revision']
    invoke(diff, 'set-files-viewed', {'paths': [path], 'viewed': True, 'revision': revision})
    native.wait_for(lambda: client.js("return document.querySelector('.diff-viewed input').checked"), 'CLI viewed updates UI')
    (directory/'repository'/path).write_text('new comparison revision\n')
    client.click("//button[normalize-space()='Refresh']", 'xpath')
    native.wait_for(lambda: state(diff)['data']['revision'] != revision and not state(diff)['data']['loading'], 'comparison revision changes')
    invoke(diff, 'set-files-viewed', {'paths': [path], 'viewed': True, 'revision': revision}, error='revision_conflict')
    assert state(diff)['data']['files'][0]['viewed'] is False
    mark('CLI Diff pagination, shared UI setters, atomic batch rejection and stale revision protection pass')

    created = command('session', 'create', '--name', 'Interaction caller', '--directory', directory/'repository', '--project', session['projectId'])
    other = created['id']
    result = command('tab', 'create', '--session', other, '--action', 'sdk')
    caller = next(s for s in result['sessions'] if s['id'] == other)['selectedTabId']
    native.wait_for(lambda: client.js('return window.__sdkValidation?.ready(arguments[0])', caller), 'SDK caller')
    listing = sdk(caller, 'list')['tabs']
    assert any(t['id'] == diff and t['sessionId'] == session['id'] for t in listing)
    assert all(t['sessionId'] == other for t in sdk(caller, 'list', {'sessionId': other})['tabs'])
    assert sdk(caller, 'capabilities', diff) == command('tab', 'capabilities', diff)
    sdk_state = sdk(caller, 'state', diff)['state']
    assert sdk_state == state(diff)
    assert sdk(caller, 'invoke', diff, 'set-files-expanded', {'paths': [path], 'expanded': False}) == {'updated': 1}
    assert state(diff)['data']['files'][0]['expanded'] is False
    sdk(caller, 'invoke', diff, 'missing', {}, error='unsupported_operation')
    command('tab', 'invoke', diff, 'missing', error='unsupported_operation')
    sdk(caller, 'state', diff, {'limit': 201}, error='invalid_arguments')
    command('tab', 'state', diff, '--args', '{"limit":201}', error='invalid_arguments')
    caps = command('tab', 'capabilities', caller)
    assert caps['interactions'] is None and not caps['stateAvailable']
    command('tab', 'state', caller, error='unsupported_operation')
    mark('CLI and SDK discovery/state/actions agree across sessions while target remains hidden; unsupported and invalid arguments agree')

    result = command('tab', 'create', '--session', other, '--action', 'files')
    files = next(s for s in result['sessions'] if s['id'] == other)['selectedTabId']
    native.wait_for(lambda: command('tab', 'capabilities', files)['stateAvailable'], 'Files provider')
    native.wait_for(lambda: not state(files)['data']['loading'], 'Files loaded')
    assert state(files)['data']['folder'] == str(directory/'repository')
    assert state(files)['data']['dirty'] is False
    assert state(files)['data']['selectedFile'] is None
    client.click(f"//section[@id='tab-panel-{files}']//button[contains(.,'hello.txt')]", 'xpath')
    native.wait_for(lambda: state(files)['data']['selectedFile'] == str(directory/'repository/hello.txt'), 'Files selected path')
    assert 'text' not in state(files)['data']
    result = command('tab', 'create', '--session', other, '--action', 'pr')
    pr = next(s for s in result['sessions'] if s['id'] == other)['selectedTabId']
    native.wait_for(lambda: command('tab', 'capabilities', pr)['stateAvailable'], 'PR provider')
    pr_state = state(pr)
    assert pr_state['data']['url'] is None and pr_state['data']['reviewCount'] == 0 and pr_state['stale']
    assert 'reviews' not in pr_state['data'] and 'token' not in pr_state['data']
    command('tab', 'invoke', pr, 'send-text', error='unsupported_operation')
    mark('Files folder/selection/editor status and unavailable PR summary serialize without file contents or review bodies')
    result = command('tab', 'create', '--session', other, '--action', 'probe')
    terminal = next(s for s in result['sessions'] if s['id'] == other)['selectedTabId']
    native.wait_for(lambda: command('tab', 'capabilities', terminal)['stateAvailable'], 'terminal provider')
    native.wait_for(lambda: 'row 599 café 日本語 🙂' in '\n'.join(state(terminal, {'lines': 500})['data']['lines']), 'Unicode scrollback')
    terminal_state = state(terminal, {'lines': 500})['data']
    assert terminal_state['returnedLines'] == 500 and terminal_state['truncated']
    assert state(terminal)['data']['returnedLines'] == 100
    client.js('window.__terminalValidation.selectAll(arguments[0])', terminal)
    before = client.state(terminal)
    state(terminal, {'lines': 500})
    after = client.state(terminal)
    assert before['selection'] == after['selection'] and before['viewportY'] == after['viewportY']
    assert invoke(terminal, 'send-text', {'text': 'alternate', 'enter': True}) == {'acknowledged': True}
    native.wait_for(lambda: state(terminal)['data']['screen'] == 'alternate', 'alternate screen')
    active = state(terminal)['data']
    assert 'ALT 日本語 🙂' in '\n'.join(active['lines']) and not any('row 599' in line for line in active['lines'])
    sdk(caller, 'invoke', terminal, 'send-text', {'text': 'normal', 'enter': True})
    native.wait_for(lambda: state(terminal)['data']['screen'] == 'normal', 'normal screen restored')
    invoke(terminal, 'send-text', {'text': 'budget', 'enter': True})
    native.wait_for(lambda: state(terminal, {'lines': 500})['data']['returnedLines'] < 500, 'terminal byte-budget truncation')
    bounded = state(terminal, {'lines': 500})['data']
    assert bounded['truncated'] and len('\n'.join(bounded['lines']).encode()) <= 48*1024
    command('tab', 'state', terminal, '--args', '{"lines":501}', error='invalid_arguments')
    invoke(terminal, 'send-text', {'text': 'x'*8192, 'enter': True}, error='limit_exceeded')
    mark('Terminal state reads Unicode/wide scrollback and active alternate screen without selection/viewport mutation; input acknowledges and enforces 8 KiB')
    client.js("window.__interactionValidation.mode(arguments[0],'state','invalid')", diff)
    command('tab', 'state', diff, error='invalid_arguments')
    client.js("window.__interactionValidation.mode(arguments[0],'state','nonJson')", diff)
    command('tab', 'state', diff, error='operation_failed')
    client.js("window.__interactionValidation.mode(arguments[0],'state','oversized')", diff)
    command('tab', 'state', diff, error='limit_exceeded')
    client.js("window.__interactionValidation.mode(arguments[0],'state','hold')", diff)
    client.js("window.__heldInteractions=Array.from({length:4},()=>window.__sdkValidation.call(arguments[0],'tabs','state',[arguments[1]]).then(value=>({value}),error=>({code:error.code})));return true", caller, diff)
    native.wait_for(lambda: client.js('return window.__interactionValidation.counts().started') == 4, 'four concurrent handlers')
    command('tab', 'state', diff, error='limit_exceeded')
    responses = client.request('POST', '/execute/async', {'script': 'const done=arguments[arguments.length-1];Promise.all(window.__heldInteractions).then(done)', 'args': []})
    assert all(r['code'] == 'timeout' for r in responses), responses
    native.wait_for(lambda: client.js('return window.__interactionValidation.counts().aborted') == 4, 'timeouts abort signals')
    client.js("window.__interactionValidation.mode(arguments[0],'state','')", diff)
    assert state(diff)['data']['totalFiles'] == 2
    client.js("window.__interactionValidation.mode(arguments[0],'set-files-expanded','hold')", diff)
    timeout = invoke(diff, 'set-files-expanded', {'paths': [path], 'expanded': True}, error='timeout')
    assert 'may have executed' in timeout['message']
    client.js("window.__interactionValidation.mode(arguments[0],'state','hold')", diff)
    client.js("window.__detachInteraction=window.__sdkValidation.call(arguments[0],'tabs','state',[arguments[1]]).then(value=>({value}),error=>({code:error.code}));return true", caller, diff)
    native.wait_for(lambda: client.js('return window.__interactionValidation.counts().started') == 6, 'handler before close')
    command('tab', 'close', diff)
    detached = client.request('POST', '/execute/async', {'script': 'const done=arguments[arguments.length-1];window.__detachInteraction.then(done)', 'args': []})
    assert detached['code'] == 'unavailable', detached
    mark('Native invalid/oversized results, four-request pressure, timeout abort signals, ambiguous action errors and target-detach cancellation pass')
    command('tab', 'state', diff, error='unavailable')
    sdk(caller, 'state', diff, error='unavailable')
    client.js("window.__interactionValidation.mode(arguments[0],'state','hold')", terminal)
    pending = subprocess.Popen([str(cli), '--json', 'tab', 'state', str(terminal)], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    native.wait_for(lambda: client.js('return window.__interactionValidation.counts().started') == 7, 'pending request before reload')
    client.request('POST', '/refresh', {})
    stdout, stderr = pending.communicate(timeout=8)
    assert json.loads(stdout or stderr)['error']['code'] == 'unavailable'
    native.wait_for(lambda: state(terminal)['data']['status']['state'] == 'running', 'provider reattached after reload')
    mark('Closed targets fail consistently; reload restores providers and retains PTY')
