"""Profile repeated discussion snapshots and validate scoped updates in WebKitGTK."""
import json
import subprocess


def check(client, native, directory, env, session, tab, mark, cli):
    def invoke(action, args):
        result = subprocess.run([str(cli), '--json', 'tab', 'invoke', str(tab), action,
                                 '--args', json.dumps(args)], env=env, capture_output=True, text=True, timeout=15)
        response = json.loads(result.stdout or result.stderr)
        assert response['ok'], response
        return response['data']
    root = str(directory/'repository')
    threads = []
    for i in range(40):
        for slot, side in enumerate([None, 'old', 'new', 'new']):
            location = {'root': root, 'path': f'zz-comments-{i:02}.txt', 'context': []}
            if side:
                location.update(side=side, line=10, targetOffset=0,
                                context=[f'{"before" if side == "old" else "changed"} row 10'])
            threads.append({'id': f'thread-{i}-{slot}', 'original': location, 'location': location,
                            'orphan': False, 'comments': [{'id': f'comment-{i}-{slot}',
                            'text': f'Discussion {i}-{slot}: ' + 'review text ' * 40,
                            'origin': 'User', 'at': '2026-11-16T10:00:00.000Z'}]})
    client.js('localStorage.setItem(arguments[0],arguments[1])',
              f'ongleet.diff-comments.v1.{session["id"]}.{tab}', json.dumps({'version': 1, 'threads': threads}))
    client.request('POST', '/refresh', {})
    native.wait_for(lambda: client.js("return document.querySelectorAll('.diff-file .diff-discussion').length===160 && !document.querySelector('.diff-panel [role=status]')"), '160 attached discussions')
    assert not client.js("return document.body.textContent.includes('Orphan comments')")
    mark('Forty expanded patches restore file and old/new line discussions, including two threads on one line')
    metrics = client.request('POST', '/execute/async', {'script': """
        const done=arguments[arguments.length-1];
        const stringify=JSON.stringify;
        let snapshots=0;
        JSON.stringify=function(value,...args) {
            if(Array.isArray(value) && value.length===4 && Array.isArray(value[2]) &&
               value[2].every(thread=>thread && Array.isArray(thread.comments))) snapshots++;
            return stringify.call(this,value,...args);
        };
        const filterInput=document.querySelector('.diff-layout aside label input');
        const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;
        const frame=()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));
        (async()=>{
            const samples=[];
            for(let i=0;i<8;i++) {
                await frame();
                const start=performance.now();
                set.call(filterInput,i%2 ? '' : 'zz');
                filterInput.dispatchEvent(new Event('input',{bubbles:true}));
                await frame();samples.push(Math.round(performance.now()-start));
            }
            JSON.stringify=stringify;
            done({samplesMs:samples,serializedFileSnapshots:snapshots});
        })().catch(error=>{JSON.stringify=stringify;done({error:String(error),input:filterInput?.outerHTML,stack:error.stack})});
    """, 'args': []})
    assert 'error' not in metrics, metrics
    (directory/'discussion-profile.json').write_text(json.dumps(metrics, indent=2))
    print('Discussion profile: ' + json.dumps(metrics), flush=True)
    mark('Repeated filename filtering preserves all attached discussions')
    # Keep a sibling reply draft and table alive while another file receives a reply.
    client.js("""
        const other=document.querySelector('[data-diff-path="zz-comments-01.txt"]');
        window.otherTable=other.querySelector('table');
        other.querySelector('textarea').value='Sibling unsent reply';
    """)
    invoke('reply-comment', {'threadId': 'thread-0-2', 'text': 'Updated indexed discussion'})
    native.wait_for(lambda: client.js("return document.querySelector('[data-diff-path=\"zz-comments-00.txt\"]')?.textContent.includes('Updated indexed discussion')"), 'reply on indexed line')
    assert client.js("""
        const other=document.querySelector('[data-diff-path="zz-comments-01.txt"]');
        return other.querySelector('table')===window.otherTable && other.querySelector('textarea').value==='Sibling unsent reply';
    """)
    client.js("document.querySelector('.diff-discussion[aria-label=\"Thread thread-0-3\"] textarea').value='Same line unsent reply'")
    invoke('delete-comment-thread', {'threadId': 'thread-0-2'})
    native.wait_for(lambda: client.js("return !document.querySelector('[data-diff-path=\"zz-comments-00.txt\"]').textContent.includes('Updated indexed discussion')"), 'indexed thread deletion')
    assert client.js("return document.querySelectorAll('[data-diff-path=\"zz-comments-00.txt\"] .diff-widget .diff-discussion').length===2")
    assert client.js("return document.querySelector('.diff-discussion[aria-label=\"Thread thread-0-3\"] textarea').value==='Same line unsent reply'")
    mark('Reply and deletion update the target line while sibling tables and unsent drafts survive')
    selector='.diff-discussion[aria-label="Thread thread-0-3"]'
    client.click(selector+' .diff-message button')
    client.fill(selector+' textarea[aria-label="Save comment"]', 'Edited indexed discussion')
    client.click("//section[@aria-label='Thread thread-0-3']//button[normalize-space()='Save comment']", 'xpath')
    native.wait_for(lambda: client.js("return document.querySelector(arguments[0]).textContent.includes('Edited indexed discussion') && !document.querySelector(arguments[0]+' textarea[aria-label=\"Save comment\"]')", selector), 'indexed UI edit')
    assert client.js("return document.querySelector('[data-diff-path=\"zz-comments-01.txt\"] textarea').value==='Sibling unsent reply'")
    mark('Editing a line comment publishes its new text and preserves sibling drafts')
