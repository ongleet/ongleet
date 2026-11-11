"""Exercise color-query replies and replay suppression through a real PTY."""
import json
import os
from pathlib import Path
import select
import subprocess
import sys
import time


def fixture():
    import tty
    tty.setraw(0)
    def query():
        data = (b'\x1b]666;vte.shell.precmd!\x1b\\'
                b'\x1b]7501;state=working:app=codex\x07'
                b'\x1b]10;?\x07\x1b]11;?\x1b\\')
        for byte in data:
            os.write(1, bytes([byte]))
            time.sleep(.001)
    query()
    while True:
        if Path('osc-query-again').exists():
            Path('osc-query-again').unlink()
            query()
        if select.select([0], [], [], .05)[0]:
            data = os.read(0, 8192)
            if not data:
                return
            with Path('osc-replies').open('ab') as file:
                file.write(data)


def check(client, native, directory, env, session, mark, cli):
    folder = directory / 'repository'
    def command(*args):
        result = subprocess.run([str(cli), '--json', *map(str, args)], env=env,
                                capture_output=True, text=True, timeout=12)
        response = json.loads(result.stdout or result.stderr)
        assert response['ok'], response
        return response['data']
    client.js("""
        window.oscWarnings=[];
        const log=console.log.bind(console);
        console.log=(...args)=>{
            if(args[0]==='[ghostty-vt]')window.oscWarnings.push(String(args[1]));
            log(...args);
        };
    """)
    created = command('tab', 'create', '--session', session['id'], '--action', 'osc')
    tab = next(s for s in created['sessions'] if s['id'] == session['id'])['selectedTabId']
    expected = b'\x1b]10;rgb:dcdc/e3e3/ecec\x07\x1b]11;rgb:1212/1616/1d1d\x1b\\'
    replies = folder / 'osc-replies'
    native.wait_for(lambda: replies.exists() and len(replies.read_bytes()) >= len(expected), 'real PTY color replies')
    assert replies.read_bytes() == expected, replies.read_bytes()
    assert not client.js('return window.oscWarnings'), client.js('return window.oscWarnings')
    mark('Fragmented OSC color queries receive exact theme-color replies through a real PTY without known warnings')
    command('session', 'create', '--name', 'Foreground', '--directory', folder, '--project', session['projectId'])
    native.wait_for(lambda: client.js('return document.getElementById(arguments[0])?.hidden', f'tab-panel-{tab}'), 'hidden terminal')
    (folder / 'osc-query-again').touch()
    native.wait_for(lambda: len(replies.read_bytes()) >= len(expected) * 2, 'hidden terminal replies')
    assert replies.read_bytes() == expected * 2
    mark('Hidden terminals answer queries without requiring focus or animation frames')
    client.request('POST', '/refresh', {})
    native.wait_for(lambda: (s := client.state(tab)) and s['sessionId'] and not s['replaying'], 'reattachment replay complete')
    time.sleep(.3)
    assert replies.read_bytes() == expected * 2, 'Replayed queries must not send duplicate input'
    mark('WebView replay restores the live terminal without resending historical color replies')


if __name__ == '__main__':
    fixture()
