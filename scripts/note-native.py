"""Note acceptance checks called by check-diff-native.py --notes-only."""
import base64
import json
import subprocess


def check(client, native, directory, env, session, mark, cli):
    def command(*args, error=None):
        result = subprocess.run([str(cli), '--json', *map(str, args)], env=env,
                                text=True, capture_output=True, timeout=12)
        response = json.loads(result.stdout or result.stderr)
        if error:
            assert not response['ok'] and response['error']['code'] == error, response
            return
        assert response['ok'], response
        return response['data']
    created = command('tab', 'create', '--session', session['id'], '--action', 'note')
    tab = next(s for s in created['sessions'] if s['id'] == session['id'])['selectedTabId']
    def state(args=None):
        return command('tab', 'state', tab, '--args', json.dumps(args or {}))['state']
    def ready():
        value = state()
        return value['data'] if not value['stale'] else None
    def invoke(action, args, **kwargs):
        return command('tab', 'invoke', tab, action, '--args', json.dumps(args), **kwargs)
    native.wait_for(lambda: command('tab', 'capabilities', tab)['stateAvailable'], 'Note provider')
    first = native.wait_for(ready, 'initial note autosave')
    assert len(first['notes']) == 1 and first['notes'][0]['name'] == 'Note 1', first
    first_id = first['selectedNoteId']
    assert first['text'] == ''
    assert client.js("return !!document.querySelector('.note-sidebar button[aria-current=true]')")
    # Exercise the CodeMirror input path through the native driver.
    client.click('.note-markdown-editor .cm-content')
    client.keys('# Session TODO\n- [ ] Review')
    native.wait_for(lambda: (data if (data := ready()) and data['text'] == '# Session TODO\n- [ ] Review' else None), 'UI autosave')
    client.click("//button[normalize-space()='Add note']", 'xpath')
    second = native.wait_for(lambda: (data if (data := ready()) and len(data['notes']) == 2 else None), 'second note')
    second_id = second['selectedNoteId']
    assert second_id != first_id and second['text'] == ''
    client.js("const el=document.querySelector('.note-name input');el.focus();Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,'Agent memory');el.dispatchEvent(new Event('input',{bubbles:true}));")
    client.click('.note-markdown-editor .cm-content')
    native.wait_for(lambda: (data if (data := ready()) and data['notes'][1]['name'] == 'Agent memory' else None), 'UI rename')
    data = ready()
    result = invoke('set-text', {'noteId': second_id, 'revision': data['revision'], 'text': 'Remember this decision'})
    invoke('set-text', {'noteId': first_id, 'revision': data['revision'], 'text': 'Stale overwrite'}, error='revision_conflict')
    assert result['noteId'] == second_id
    client.click("//div[@class='note-list']/button[normalize-space()='Note 1']", 'xpath')
    native.wait_for(lambda: (data if (data := ready()) and data['selectedNoteId'] == first_id else None), 'select first note')
    assert ready()['text'] == '# Session TODO\n- [ ] Review'
    assert state({'noteId': second_id})['data']['text'] == 'Remember this decision'
    assert ready()['selectedNoteId'] == first_id
    mark('Native sidebar creates/selects notes; UI rename and Markdown autosave preserve independent text; CLI reads by ID and rejects stale writes')
    data = ready()
    added = invoke('create-note', {'revision': data['revision'], 'name': 'CLI note'})
    invoke('rename-note', {'revision': added['revision'], 'noteId': added['noteId'], 'name': 'Renamed by agent'})
    data = ready()
    invoke('select-note', {'revision': data['revision'], 'noteId': second_id})
    native.wait_for(lambda: client.js("return document.querySelector('.note-name input').value==='Agent memory'"), 'CLI selection updates UI')
    invoke('select-note', {'revision': ready()['revision'], 'noteId': 'missing'}, error='invalid_arguments')
    assert client.js("return document.querySelector('.note-name').getBoundingClientRect().height < 80")
    (directory/'notes.png').write_bytes(base64.b64decode(client.request('GET','/screenshot')))
    client.request('POST', '/refresh', {})
    native.wait_for(lambda: command('tab', 'capabilities', tab)['stateAvailable'], 'reloaded Note provider')
    data = native.wait_for(ready, 'reloaded notes')
    assert len(data['notes']) == 3 and data['selectedNoteId'] == second_id
    assert data['text'] == 'Remember this decision'
    assert data['notes'][2]['name'] == 'Renamed by agent'
    mark('Agent create/rename/select actions update UI; reload retains names, contents and selected note')
    command('session', 'archive', session['id'], '--confirm')
    native.wait_for(lambda: client.js("return !document.querySelector('.note-panel')"), 'archived notebook unmounted')
    archives = client.invoke('archives')
    archive = next(item for item in archives['entries'] if item['kind'] == 'session')
    command('archive', 'restore', archive['id'])
    client.request('POST', '/refresh', {})
    native.wait_for(lambda: command('tab', 'capabilities', tab)['stateAvailable'], 'restored Note provider')
    data = native.wait_for(ready, 'restored notebook')
    assert data['text'] == 'Remember this decision' and len(data['notes']) == 3
    mark('Session archive/restore retains every note and the selected note')
