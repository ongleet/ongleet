"""Real PTY and CLI acceptance for opt-in on-demand terminal attachment."""
import json
import subprocess
import time


def check(client, native, directory, env, session, mark, cli):
    folder = directory / 'repository'
    starts = folder / 'demand-starts'

    def command(*args, error=None):
        result = subprocess.run([str(cli), '--json', *map(str, args)], env=env,
                                capture_output=True, text=True, timeout=12)
        response = json.loads(result.stdout or result.stderr)
        if error:
            assert not response['ok'] and response['error']['code'] == error, response
            return response
        assert response['ok'], response
        return response['data']

    other = command('session', 'create', '--name', 'Foreground', '--directory', folder,
                    '--project', session['projectId'])['id']
    created = command('tab', 'create', '--session', session['id'], '--action', 'demand')
    target = next(s for s in created['sessions'] if s['id'] == session['id'])['selectedTabId']
    assert created['selectedId'] == other
    native.wait_for(lambda: client.js('return !!document.getElementById(arguments[0])', f'tab-{target}'), 'hidden lazy tab')
    caps = command('tab', 'capabilities', target)
    assert caps['available'] and caps['stateAvailable'] and 'send-text' in caps['availableActions'], caps
    command('tab', 'invoke', target, 'send-text', '--args', '{}', error='invalid_arguments')
    time.sleep(.3)
    assert not starts.exists(), 'capabilities and rejected actions must not launch a process'
    assert client.js('return !document.getElementById(arguments[0])', f'tab-panel-{target}')
    mark('Hidden on-demand terminal has no emulator/process; discovery and invalid requests do not wake it')

    calls = [subprocess.Popen([str(cli), '--json', 'tab', 'invoke', str(target), 'send-text', '--args',
                               json.dumps({'text': text, 'enter': True})], env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
             for text in ['A' * 1000, 'B' * 1000]]
    for process in calls:
        out, err = process.communicate(timeout=12)
        response = json.loads(out or err)
        assert response['ok'] and response['data'] == {'acknowledged': True}, response
    native.wait_for(lambda: starts.exists() and len(starts.read_text().splitlines()) == 1, 'one shared PTY launch')
    inputs = folder / 'demand-input'
    native.wait_for(lambda: inputs.exists() and len(inputs.read_text().splitlines()) == 2, 'queued input delivery')
    assert sorted(inputs.read_text().splitlines()) == ['A' * 1000, 'B' * 1000], 'actions must not interleave'
    snapshot = client.invoke('sessions')
    assert snapshot['selectedId'] == other, 'API wake must not select the target session'
    assert client.js('return document.getElementById(arguments[0]).hidden', f'tab-panel-{target}')
    assert command('tab', 'state', target)['state']['data']['status']['state'] == 'running'
    mark('Concurrent API actions wake one hidden PTY, deliver complete ordered messages, and leave selection unchanged')

    client.request('POST', '/refresh', {})
    native.wait_for(lambda: client.js('return !!document.getElementById(arguments[0])', f'tab-panel-{target}'), 'live PTY reattached after WebView reload')
    assert len(starts.read_text().splitlines()) == 1
    assert command('tab', 'state', target)['state']['data']['status']['state'] == 'running'
    mark('WebView reload reconnects an already running hidden terminal without relaunching it')
    command('tab', 'close', target, '--confirm')
    command('tab', 'state', target, error='unavailable')
    assert len(starts.read_text().splitlines()) == 1
    mark('Closed terminal rejects API wake and does not launch again')

    created = command('tab', 'create', '--session', session['id'], '--action', 'demand')
    shown = next(s for s in created['sessions'] if s['id'] == session['id'])['selectedTabId']
    time.sleep(.3)
    assert len(starts.read_text().splitlines()) == 1
    client.click("//button[contains(@class,'session-choice')][.//span[normalize-space()='Diff test']]", 'xpath')
    native.wait_for(lambda: len(starts.read_text().splitlines()) == 2, 'first display starts deferred terminal')
    client.click("//button[contains(@class,'session-choice')][.//span[normalize-space()='Foreground']]", 'xpath')
    assert command('tab', 'state', shown)['state']['data']['status']['state'] == 'running'
    assert len(starts.read_text().splitlines()) == 2
    mark('First display starts another deferred terminal; hiding it preserves the running process')

    created = command('tab', 'create', '--session', session['id'], '--action', 'demand')
    immediate = next(s for s in created['sessions'] if s['id'] == session['id'])['selectedTabId']
    assert command('tab', 'state', immediate)['state']['data']['status']['state'] == 'running'
    native.wait_for(lambda: len(starts.read_text().splitlines()) == 3, 'immediate API wake')
    assert client.invoke('sessions')['selectedId'] == other
    mark('API immediately after hidden tab creation wakes it without waiting for host listener installation')
