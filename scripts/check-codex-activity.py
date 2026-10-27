"""Validate ong-codex activity in isolated WebKitGTK using a local Codex fixture.

Exercises actual wrapper processes, hooks, private control socket and sidebar.
Does not submit prompts to a model or certify real Codex hook delivery.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--application', required=True)
parser.add_argument('--cli', required=True)
parser.add_argument('--driver', required=True)
args = parser.parse_args()
spec = importlib.util.spec_from_file_location('native', ROOT / 'scripts/check-native.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
spec = importlib.util.spec_from_file_location('plugins', ROOT / 'scripts/plugin-native-checks.py')
plugins = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugins)
directory = Path(tempfile.mkdtemp(prefix='ong-codex-activity-'))
directory.chmod(0o700)
tools = directory / 'tools'
tools.mkdir()
(tools / 'codex').write_text('''#!/usr/bin/python3
import json, os, pathlib, subprocess, sys, time, tomllib
root = pathlib.Path(os.environ['TERMINAL_VALIDATION_RUN'])
tab = os.environ['ONGLEET_TAB_ID']
settings = {}
for i, arg in enumerate(sys.argv[:-1]):
    if arg == '-c': settings.setdefault('hooks', {}).update(tomllib.loads(sys.argv[i+1]).get('hooks', {}))
hooks = settings['hooks']
def emit(event):
    name = event['hook_event_name']
    payload = {'session_id': 'fixture-'+tab, 'turn_id':'turn-1', **event}
    command = hooks[name][0]['hooks'][0]['command']
    result = subprocess.run(command, shell=True, input=json.dumps(payload), text=True, capture_output=True, timeout=6)
    if result.returncode: raise RuntimeError(result.stderr)
events = root / ('events-'+tab+'.json')
cursor = len(json.loads(events.read_text())) if events.exists() else 0
emit({'hook_event_name':'SessionStart'})
(root / ('started-'+tab+'.json')).write_text(json.dumps({'pid':os.getpid(), 'launch':os.environ['ONGLEET_CODEX_LAUNCH']}))
while True:
    entries = json.loads(events.read_text()) if events.exists() else []
    for event in entries[cursor:]:
        if event['hook_event_name'] == 'Exit': sys.exit(0)
        emit(event)
        cursor += 1
    time.sleep(.03)
''')
(tools / 'codex').chmod(0o700)
env = dict(os.environ, XDG_DATA_HOME=str(directory / 'data'),
           XDG_CONFIG_HOME=str(directory / 'config'), XDG_CACHE_HOME=str(directory / 'cache'),
           XDG_RUNTIME_DIR=f'/run/user/{os.getuid()}', GDK_BACKEND='x11',
           TAURI_WEBVIEW_AUTOMATION='true', TERMINAL_VALIDATION_RUN=str(directory),
           PATH=str(tools) + os.pathsep + os.environ['PATH'])
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
client = native.WebDriver(f'http://127.0.0.1:{port}')
checks = []

def cli(*arguments):
    result = subprocess.run([args.cli, '--json', *map(str, arguments)], env=env,
                            capture_output=True, text=True, timeout=15, check=True)
    return json.loads(result.stdout)['data']

def mark(description):
    checks.append(description)
    print('PASS:', description, flush=True)

def send(tab, name, **fields):
    path = directory / f'events-{tab}.json'
    entries = json.loads(path.read_text()) if path.exists() else []
    entries.append({'hook_event_name': name, **fields})
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(entries))
    temporary.replace(path)

def activity(tab, expected):
    def observed():
        snapshot = client.invoke('sessions')
        return any(t['id'] == tab and t.get('codexActivity') == expected
                   for s in snapshot['sessions'] for t in s['tabs'])
    native.wait_for(observed, f'tab {tab}: {expected}')

def badge(expected):
    colors = {'Working':'rgb(96, 165, 250)', 'Question':'rgb(245, 158, 11)', 'Approval':'rgb(249, 115, 22)', 'Idle':'rgb(34, 197, 94)', 'Stopped':'rgb(148, 163, 184)'}
    native.wait_for(lambda: client.js("return document.querySelector('.session-tag[title=Codex] .session-tag-dot')?.style.backgroundColor === arguments[0]", colors[expected]), f'sidebar Codex dot {expected}')

with (directory / 'driver.log').open('w') as log:
    driver = subprocess.Popen([args.driver, '--host=127.0.0.1', f'--port={port}'],
                              env=env, stdout=log, stderr=log, start_new_session=True)
    try:
        native.wait_for(lambda: client.request('GET', '/status', session=False), 'driver')
        started = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {
            'webkitgtk:browserOptions': {'binary': str(Path(args.application).resolve())}}}}, session=False)
        client.session = started['sessionId']
        native.wait_for(lambda: client.js("return !!document.querySelector('footer button:not(:disabled)')"), 'app ready')
        project = cli('project', 'create', '--name', 'Codex fixture', '--root', directory)['id']
        config = directory / f'config/ongleet/projects/{project}/actions.yaml'
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text(json.dumps({'actions': [{'id':'codex', 'name':'Codex fixture', 'kind':'terminal', 'config':{'executable':'ong-codex','args':[]}}]}))
        session = cli('session', 'create', '--project', project, '--name', 'Activity', '--directory', directory)['id']
        snapshot = cli('session', 'config', session)
        first = snapshot['selectedTabId']
        native.wait_for(lambda: (directory / f'started-{first}.json').exists(), 'first wrapper')
        activity(first, 'idle')
        badge('Idle')
        cli('session', 'tag-add', session, '--name', 'Backend', '--dot', '#112233')
        plugins.sdk(client, first, 'session', 'addTag', {'name':'SDK tag','background':'#ffffff','dot':'#445566','priority':200})
        native.wait_for(lambda: client.js("return document.querySelector('.session-tags')?.innerText.includes('SDK tag')"), 'SDK tag refresh')
        listed = plugins.sdk(client, first, 'session', 'listTags')
        assert [tag['name'] for tag in listed] == ['SDK tag', 'Codex', 'Backend']
        assert client.js("return document.querySelector('.session-tag[title=\"SDK tag\"]').style.color") == 'rgb(0, 0, 0)'
        plugins.sdk(client, first, 'session', 'addTag', {'name':'SDK tag','background':'#123456','priority':200})
        assert len([tag for tag in plugins.sdk(client, first, 'session', 'listTags') if tag['name'] == 'SDK tag']) == 1
        plugins.sdk(client, first, 'session', 'removeTag', 'SDK tag')
        created = cli('tab', 'create', '--session', session, '--action', 'codex')
        second = next(s for s in created['sessions'] if s['id'] == session)['selectedTabId']
        native.wait_for(lambda: (directory / f'started-{second}.json').exists(), 'second wrapper')
        activity(second, 'idle')
        send(first, 'UserPromptSubmit')
        activity(first, 'working')
        badge('Working')
        send(first, 'PreToolUse', tool_name='request_user_input', tool_use_id='question')
        activity(first, 'question')
        send(second, 'UserPromptSubmit')
        activity(second, 'working')
        badge('Question')
        send(first, 'PostToolUse', tool_name='Bash', tool_use_id='unrelated')
        send(second, 'PermissionRequest', tool_name='Bash', tool_use_id='approval')
        activity(second, 'approval')
        activity(first, 'question')
        badge('Question')
        send(first, 'PostToolUse', tool_name='request_user_input', tool_use_id='question')
        activity(first, 'working')
        badge('Approval')
        send(second, 'PostToolUse', tool_name='Bash', tool_use_id='approval')
        activity(second, 'working')
        badge('Working')
        mark('Real wrapper hooks and socket update Working/Question/Approval; concurrent tabs preserve waits')
        send(first, 'Stop')
        send(second, 'Interrupt')
        activity(first, 'idle')
        activity(second, 'idle')
        badge('Idle')
        before = json.loads((directory / f'started-{first}.json').read_text())
        send(first, 'Exit')
        native.wait_for(lambda: json.loads((directory / f'started-{first}.json').read_text())['launch'] != before['launch'], 'automatic wrapper restart')
        activity(first, 'idle')
        send(first, 'UserPromptSubmit')
        activity(first, 'working')
        stale_env = dict(env, ONGLEET_CODEX_DATA=str(directory / 'data/ongleet'),
                         ONGLEET_SESSION_ID=str(session), ONGLEET_TAB_ID=str(first),
                         ONGLEET_CODEX_LAUNCH=before['launch'])
        subprocess.run([args.application, '--internal-codex-record'], input=json.dumps({'hook_event_name':'Stop','session_id':f'fixture-{first}'}), text=True, env=stale_env, check=True, timeout=5)
        activity(first, 'working')
        badge('Working')
        client.request('POST', '/refresh', {})
        badge('Working')
        assert [tag['name'] for tag in cli('session', 'tags', session)['tags']] == ['Codex', 'Backend']
        assert next(tag for tag in cli('session', 'tags', session)['tags'] if tag['name'] == 'Backend') == {'name':'Backend','dot':'#112233','priority':0}
        mark('Stop/Interrupt reset idle; wrapper restart rejects stale hooks; reload retains activity and manual tags')
        cli('tab', 'close', first, '--confirm')
        badge('Idle')
        send(second, 'SessionEnd')
        activity(second, 'stopped')
        badge('Stopped')
        mark('Closed tabs leave the aggregate; SessionEnd displays Stopped')
        (directory / 'screenshot.png').write_bytes(__import__('base64').b64decode(client.request('GET', '/screenshot')))
    except Exception:
        try:
            (directory / 'failure.json').write_text(json.dumps(client.js('return {body:document.body.innerText, screens:window.__terminalValidation?.ids?.()}'), indent=2))
        except Exception:
            pass
        raise
    finally:
        try:
            cli('app', 'quit', '--confirm')
        except Exception:
            pass
        if client.session:
            try:
                client.request('DELETE', '', {})
            except Exception:
                pass
        driver.terminate()
        driver.wait(timeout=5)
        survivors = []
        for path in directory.glob('started-*.json'):
            pid = json.loads(path.read_text())['pid']
            if Path(f'/proc/{pid}').exists():
                survivors.append(pid)
        (directory / 'report.json').write_text(json.dumps({'checks':checks,'survivors':survivors,'limitation':'Fixture hook delivery; actual Codex request_user_input is not certified.'}, indent=2))
        print('Artifacts:', directory, flush=True)
        assert not survivors, f'Fixture survivors: {survivors}'
