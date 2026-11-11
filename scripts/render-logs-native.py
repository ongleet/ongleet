"""Verify the real SDK debug gate and console output, without recording text."""
import json
import subprocess


def check(client, native, env, session, mark, cli):
    client.js("""
        window.renderLogEvents=[];
        const info=console.info.bind(console);
        console.info=(...args)=>{
            if(args[0]==='[diff-diagnostics]') {
                try { window.renderLogEvents.push(...JSON.parse(args[1])); } catch {}
                window.renderLogEvents=window.renderLogEvents.slice(-500);
            }
            info(...args);
        };
    """)
    result = subprocess.run([str(cli), '--json', 'tab', 'create', '--session', str(session['id']),
                             '--action', 'diff'], env=env, capture_output=True, text=True, timeout=12)
    response = json.loads(result.stdout or result.stderr)
    assert response['ok'], response
    native.wait_for(lambda: client.js("return window.renderLogEvents.some(e=>e.event==='enabled')"), 'diagnostics enabled through actual environment capability')
    native.wait_for(lambda: client.js("return window.renderLogEvents.some(e=>e.component==='DiffContent' && e.event==='render-committed')"), 'Diff render logs')
    mark('ONGLEET_DEBUG=1 enables real Diff diagnostics through the declared environment capability')
    native.wait_for(lambda: client.js("return !!document.querySelector('.diff-toolbar select')"), 'Diff toolbar')
    client.js("const checkbox=document.querySelector('.diff-uncommitted-field input');if(!checkbox.checked)checkbox.click()")
    client.click("//button[normalize-space()='Compare']", 'xpath')
    native.wait_for(lambda: client.js("return !!document.querySelector('.diff-file') && !document.querySelector('.diff-panel [role=status]')"), 'comparison')
    client.click("//article[contains(@class,'diff-file')]//header//button[normalize-space()='Comment']", 'xpath')
    client.click('.diff-comment-composer textarea')
    marker = 'render-private-probe'
    client.keys(marker)
    native.wait_for(lambda: client.js("return window.renderLogEvents.some(e=>e.event==='textarea-input')"), 'input correlation log')
    native.wait_for(lambda: client.js("return window.renderLogEvents.some(e=>e.component==='Composer')"), 'Composer render log')
    native.wait_for(lambda: client.js("return window.renderLogEvents.some(e=>e.component==='App') && window.renderLogEvents.some(e=>e.component==='SessionPane')"), 'parent render logs')
    events = client.js('return window.renderLogEvents')
    assert marker not in json.dumps(events)
    for event in events:
        if event['event'] == 'render-committed':
            assert set(event) == {'atMs', 'event', 'component', 'instance', 'render', 'changed'}, event
            assert isinstance(event['changed'], list)
    mark('Input, Composer, Diff and parent render logs are emitted without textarea text or prop values')
