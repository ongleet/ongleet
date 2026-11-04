"""Selected Files/Diff text to named terminal tabs, on the private native fixture."""
import json
import subprocess


def check(client, native, directory, env, session, diff, mark, cli):
    def command(*args):
        result = subprocess.run([str(cli), '--json', *map(str, args)], env=env, text=True, capture_output=True, timeout=15)
        response = json.loads(result.stdout or result.stderr)
        assert response['ok'], response
        return response['data']
    def create(action, name=None):
        args = ['tab','create','--session',session['id'],'--action',action]
        if name: args += ['--name',name]
        result = command(*args)
        return next(row for row in result['sessions'] if row['id']==session['id'])['selectedTabId']
    def state(tab): return command('tab','state',tab)['state']['data']
    def select_diff():
        return client.js("""
            const cell=document.querySelector('[data-diff-path="hello.txt"] .diff-code-insert');
            const walker=document.createTreeWalker(cell,NodeFilter.SHOW_TEXT);const text=walker.nextNode();
            const range=document.createRange();range.setStart(text,0);range.setEnd(text,7);
            const selection=getSelection();selection.removeAllRanges();selection.addRange(range);
            return selection.toString();
        """)
    def clear_recipient_files():
        for name in ['recipient-ready','recipient-input']:
            (directory/'repository'/name).unlink(missing_ok=True)
    clear_recipient_files()
    recipient = create('comment-recipient','Code agent')
    native.wait_for(lambda:(directory/'repository'/'recipient-ready').exists(),'raw recipient ready')
    client.click(f'#tab-{diff}')
    native.wait_for(lambda:not state(diff)['loading'],'Diff loaded')
    client.js('getSelection().removeAllRanges()')
    client.click('.diff-toolbar .selection-send > button')
    assert client.js("return !document.querySelector('dialog[open]') && document.querySelector('.diff-toolbar [role=alert]').textContent.includes('Select')")
    client.js("const cells=document.querySelectorAll('.diff-viewer td.diff-code');const range=document.createRange();range.setStart(cells[0].firstChild,0);range.setEnd(cells[cells.length-1].firstChild,3);getSelection().removeAllRanges();getSelection().addRange(range)")
    client.click('.diff-toolbar .selection-send > button')
    assert client.js("return !document.querySelector('dialog[open]') && document.querySelector('.diff-toolbar [role=alert]').textContent.includes('one diff file')")
    assert not (directory/'repository'/'recipient-input').exists()
    mark('Empty and cross-file selections are rejected before any terminal write')
    assert select_diff()=='changed'
    client.click('.diff-toolbar .selection-send > button')
    native.wait_for(lambda:client.js("return document.querySelector('dialog[aria-label=\"Send selected code\"] select')?.options.length===3"),'multiple recipients')
    assert client.js("return document.querySelector('dialog[open] .selection-preview').textContent==='changed' && document.querySelector('dialog[open] button[type=submit]').disabled")
    assert client.js("return document.querySelector('dialog[open]').textContent.includes('new:1') && !document.querySelector('dialog[open]').textContent.includes('old:1')")
    client.click("//dialog[@open]//button[normalize-space()='Cancel']",'xpath')
    assert not (directory/'repository'/'recipient-input').exists()
    command('tab','close',session['tabs'][0]['id'],'--confirm')
    assert select_diff()=='changed'
    client.click('.diff-toolbar .selection-send > button')
    native.wait_for(lambda:client.js("return document.querySelector('dialog[open] button[type=submit]')?.textContent==='Send to Code agent'"),'sole recipient direct button')
    assert client.js("return !document.querySelector('dialog[open] select')")
    client.fill('dialog[open] textarea','Why did this change?')
    client.click('dialog[open] button[type=submit]')
    native.wait_for(lambda:(directory/'repository'/'recipient-input').exists(),'Diff selection delivery')
    payload=(directory/'repository'/'recipient-input').read_bytes()
    assert payload.endswith(b'\r') and len(payload)<=8192
    assert f'Diff tab {diff}\nFile: {directory}/repository/hello.txt\nLocation: new:1\n'.encode() in payload
    assert b'\nSelected text:\nchanged\n\nComment / question:\nWhy did this change?\n' in payload
    assert b'Nearby code context:' in payload and b'+changed long line' in payload
    assert b'CommentComment' not in payload and b'Location: old:1' not in payload
    native.wait_for(lambda:client.js("return !document.querySelector('dialog[open]') && document.querySelector('.diff-toolbar .selection-send [role=status]')?.textContent.includes('Write acknowledged')"),'Diff write acknowledgment')
    mark('Diff selection preserves exact partial code and new-side location, includes context/question, chooses among multiple tabs and directly addresses a sole recipient')
    command('tab','close',recipient,'--confirm')
    native.wait_for(lambda:client.js('return !document.getElementById(arguments[0])',f'tab-{recipient}'),'closed recipient')
    clear_recipient_files()
    recipient = create('comment-recipient','Editor agent')
    native.wait_for(lambda:(directory/'repository'/'recipient-ready').exists(),'second recipient ready')
    files = create('files')
    native.wait_for(lambda:command('tab','capabilities',files)['stateAvailable'],'Files provider')
    command('tab','invoke',files,'open-path','--args',json.dumps({'path':str(directory/'repository'/'hello.txt')}))
    native.wait_for(lambda:client.js("return !!document.querySelector('.cm-content')"),'editor')
    client.click('.cm-content')
    def select_all_editor():
        client.request('POST','/actions',{'actions':[{'type':'key','id':'editor-selection','actions':[{'type':'keyDown','value':'\ue009'},{'type':'keyDown','value':'a'},{'type':'keyUp','value':'a'},{'type':'keyUp','value':'\ue009'}]}]})
    select_all_editor()
    client.keys('unsaved selected value')
    native.wait_for(lambda:state(files)['dirty'],'unsaved buffer')
    select_all_editor()
    client.click('.editor-toolbar .selection-send > button')
    native.wait_for(lambda:client.js("return document.querySelector('dialog[open] button[type=submit]')?.textContent==='Send to Editor agent'"),'editor direct recipient')
    assert client.js("return document.querySelector('dialog[open] .selection-preview').textContent==='unsaved selected value'")
    assert client.js("return document.querySelector('dialog[open]').textContent.includes('Unsaved buffer')")
    client.click('dialog[open] button[type=submit]')
    native.wait_for(lambda:(directory/'repository'/'recipient-input').exists(),'editor selection delivery')
    payload=(directory/'repository'/'recipient-input').read_bytes()
    assert payload.startswith(f'Files tab {files}\nFile: {directory}/repository/hello.txt\nLocation: lines 1:1–1:23 (end exclusive)\n'.encode()),payload
    assert b'(unsaved editor buffer)' in payload
    assert b'\nSelected text:\nunsaved selected value\n' in payload
    assert b'Comment / question:' not in payload and payload.endswith(b'\r')
    assert (directory/'repository'/'hello.txt').read_text().startswith('changed long line')
    assert state(files)['dirty']
    mark('Editor sends exact unsaved buffer selection with file, range, revision and nearby context; optional question can be empty; disk and draft remain unchanged')
