"""Diff discussion acceptance on an isolated WebKitGTK fixture."""
import json
import subprocess


def check(client, native, directory, env, session, diff, mark, cli):
    def command(*args, error=None):
        result = subprocess.run([str(cli), '--json', *map(str, args)], env=env, text=True, capture_output=True, timeout=15)
        response = json.loads(result.stdout or result.stderr)
        if error:
            assert not response['ok'] and response['error']['code'] == error, response
            return
        assert response['ok'], response
        return response['data']
    def state(): return command('tab', 'state', diff)['state']['data']
    def invoke(action, args, **kw): return command('tab','invoke',diff,action,'--args',json.dumps(args),**kw)
    native.wait_for(lambda: not state()['loading'], 'loaded comparison')
    revision = state()['revision']
    invoke('add-comment', {'revision':'stale','path':'hello.txt','text':'stale'},error='revision_conflict')
    ids = invoke('add-comment', {'revision':revision,'path':'hello.txt','side':'new','line':1,'text':'Explain this line'})
    native.wait_for(lambda: client.js("return Array.from(document.querySelectorAll('.diff-widget')).some(el=>el.textContent.includes('Explain this line'))"), 'inline comment widget')
    assert state()['totalThreads'] == 1
    invoke('reply-comment', {'threadId':ids['threadId'],'text':'Agent reply'})
    native.wait_for(lambda: client.js("return document.querySelector('.diff-discussion')?.textContent.includes('Agent reply')"), 'shared reply UI')
    client.js("document.querySelector('.diff-discussion details summary').click()")
    native.wait_for(lambda: client.js("return document.querySelector('.diff-discussion details button')?.textContent==='Shell'"), 'named same-session recipient')
    terminal = session['tabs'][0]['id']
    created = command('tab','create','--session',session['id'],'--action','comment-recipient')
    recipient = next(row for row in created['sessions'] if row['id'] == session['id'])['selectedTabId']
    native.wait_for(lambda: (directory/'repository'/'recipient-ready').exists(), 'raw recipient ready')
    assert invoke('send-comment', {**ids,'targetTabId':recipient}) == {'acknowledged':True}
    native.wait_for(lambda: (directory/'repository'/'recipient-input').exists(), 'terminal delivery')
    delivered = (directory/'repository'/'recipient-input').read_bytes()
    thread = ids['threadId']
    comment = ids['commentId']
    entries = state()['comments']
    code = '\n'.join(entries[0]['original']['context'])
    expected = f"Diff tab {diff}, thread {thread}, selected comment {comment}\nLocation: {directory/'repository'}/hello.txt new 1\nReply using ongleet tab invoke {diff} reply-comment --args '{{\"threadId\":\"{thread}\",\"text\":\"your reply\"}}'. Labels describe origin, not verified identity.\nSelected (Agent):\nExplain this line\n\nAgent {entries[1]['comment']['id']}: Agent reply\n\nCaptured code:\n{code}\r"
    assert delivered == expected.encode(), (delivered, expected)
    client.click(f'#tab-{diff}')
    mark('CLI creation/replies render inline; stale revisions rejected; named terminal receives acknowledged send')
    client.fill('.diff-discussion textarea', 'UI reply')
    client.click('.diff-discussion form button')
    native.wait_for(lambda: state()['totalComments'] == 3, 'UI reply shared with API')
    assert state()['comments'][-1]['comment']['origin'] == 'User'
    client.js("window.savedSetItem=Storage.prototype.setItem;Storage.prototype.setItem=function(){throw new Error('fixture quota failure')}")
    invoke('reply-comment', {'threadId':ids['threadId'],'text':'must not save'}, error='unavailable')
    assert client.js("return Array.from(document.querySelectorAll('[role=alert]')).some(el=>el.textContent.includes('Could not save comments'))")
    client.js("Storage.prototype.setItem=window.savedSetItem")
    assert state()['totalComments'] == 3
    mark('UI replies expose User origin through API; storage failures are visible and do not mutate discussion')
    client.request('POST','/refresh',{})
    native.wait_for(lambda: command('tab','capabilities',diff)['stateAvailable'], 'provider after reload')
    native.wait_for(lambda: state()['totalComments'] == 3 and not state()['loading'], 'reload persistence')
    assert client.js("return document.body.textContent.includes('Explain this line')")
    (directory/'repository'/'hello.txt').write_text('replacement content\n')
    client.click("//button[normalize-space()='Refresh']", 'xpath')
    native.wait_for(lambda: not state()['loading'] and state()['comments'][0]['orphan'], 'orphan after removed context')
    assert client.js("return document.body.textContent.includes('Orphan comments')")
    invoke('reply-comment', {'threadId':ids['threadId'],'text':'Orphan reply'})
    client.click(f'#tab-{terminal}')
    assert state()['totalComments'] == 4
    mark('Reload persists discussion; removed code becomes visible orphan; hidden-tab orphan replies remain accessible')
    client.click(f'#tab-{diff}')
    client.click('.diff-discussion > button')
    native.wait_for(lambda: state()['totalThreads'] == 0, 'UI thread deletion')
    client.js("Array.from(document.querySelectorAll('.diff-file > header button')).find(button=>button.textContent==='Comment').click()")
    client.fill('.diff-panel > .diff-discussion textarea', 'UI file comment')
    client.click('.diff-panel > .diff-discussion form button')
    native.wait_for(lambda: state()['totalComments'] == 1, 'UI file creation')
    assert state()['comments'][0]['comment']['origin'] == 'User'
    assert 'side' not in state()['comments'][0]['location']
    mark('UI deletes orphan threads and creates file comments through the shared persistent store')
    command('session','close',session['id'],'--confirm')
    native.wait_for(lambda: client.js("return !Object.keys(localStorage).some(key=>key.startsWith(arguments[0]))", f"ongleet.diff-comments.v1.{session['id']}."), 'CLI session cleanup')
    mark('Confirmed CLI session closure clears persisted discussion records')
    second = command('session','create','--name','GUI discussion cleanup','--directory',directory/'repository','--project',session['projectId'])
    second_id = second['id']
    created = command('tab','create','--session',second_id,'--action','diff','--config',json.dumps({'mode':'branch','includeUncommitted':True}))
    second_diff = next(row for row in created['sessions'] if row['id'] == second_id)['selectedTabId']
    native.wait_for(lambda: command('tab','capabilities',second_diff)['stateAvailable'], 'second Diff provider')
    def second_state(): return command('tab','state',second_diff)['state']['data']
    native.wait_for(lambda: second_state()['comparison'] and not second_state()['loading'], 'second comparison')
    command('tab','invoke',second_diff,'add-comment','--args',json.dumps({'revision':second_state()['revision'],'path':'hello.txt','text':'file discussion'}))
    key = f'ongleet.diff-comments.v1.{second_id}.{second_diff}'
    command('tab','close',second_diff)
    assert client.js('return !!localStorage.getItem(arguments[0])', key)
    native.wait_for(lambda: client.js('return !document.getElementById(arguments[0])', f'tab-{second_diff}'), 'closed tab removed from UI')
    command('tab','open',second_diff)
    native.wait_for(lambda: command('tab','capabilities',second_diff)['stateAvailable'], 'reopened provider')
    assert second_state()['totalComments'] == 1
    client.js("localStorage.setItem('ongleet.diff-comments.v1.999999999.1','{\"version\":1,\"threads\":[]}')")
    client.request('POST','/refresh',{})
    native.wait_for(lambda: client.js("return !localStorage.getItem('ongleet.diff-comments.v1.999999999.1')"), 'startup stale records cleanup')
    native.wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Actions for GUI discussion cleanup\"]')"), 'session menu')
    client.click('[aria-label="Actions for GUI discussion cleanup"]')
    client.click("//button[normalize-space()='Archive session']", 'xpath')
    client.click("//dialog//button[normalize-space()='Confirm']", 'xpath')
    native.wait_for(lambda: client.js('return !localStorage.getItem(arguments[0])', key), 'GUI archive cleanup')
    mark('Individual tab closure retains discussion; reopen restores it; startup prunes stale sessions; GUI archive clears records')
