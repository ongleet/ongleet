"""Production Linux service checks on a private Wayland or XWayland display.

Requires Weston with XWayland, a delegated user cgroup and the bundled GUI/CLI.
No desktop clipboard, user metadata or shell configuration is touched.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


def wait_for(check, description, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(.05)
    raise RuntimeError('Timed out: ' + description)


def dead(pid):
    try:
        return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0] == 'Z'
    except FileNotFoundError:
        return True


def run(args):
    root = Path(tempfile.mkdtemp(prefix='ongleet-system-native-'))
    root.chmod(0o700)
    runtime = root / 'runtime'
    runtime.mkdir(mode=0o700)
    env = dict(os.environ, XDG_DATA_HOME=str(root / 'data'),
               XDG_CONFIG_HOME=str(root / 'config'), XDG_CACHE_HOME=str(root / 'cache'),
               XDG_RUNTIME_DIR=str(runtime), WAYLAND_DISPLAY='system-validation',
               GDK_BACKEND='wayland' if args.backend == 'wayland' else 'x11')
    env.pop('DISPLAY', None)
    for key in ['ONGLEET_SESSION_ID', 'ONGLEET_PROJECT_ID', 'ONGLEET_TAB_ID']:
        env.pop(key, None)
    configuration = root / 'config/ongleet'
    configuration.mkdir(parents=True)
    probe = '''import json,os,pathlib,subprocess,sys,time
child=subprocess.Popen(['/usr/bin/python3','-c','import time; time.sleep(120)'],start_new_session=True)
root=pathlib.Path(os.environ['SYSTEM_FIXTURE_ROOT'])
tab=os.environ['ONGLEET_TAB_ID']
(root/(tab+'.json')).write_text(json.dumps({'pid':os.getpid(),'descendant':child.pid,'cgroup':pathlib.Path('/proc/self/cgroup').read_text().strip().split('::',1)[1]}))
print('SYSTEM_READY',flush=True)
line=sys.stdin.readline()
(root/(tab+'.input')).write_text(line)
time.sleep(120)
'''
    (configuration / 'actions.yaml').write_text(json.dumps({
        'env': {'SYSTEM_FIXTURE_ROOT': str(root)},
        'actions': [{'id': 'fixture', 'name': 'Fixture', 'kind': 'terminal',
                     'config': {'executable': '/usr/bin/python3', 'args': ['-u', '-c', probe]}}]}))
    report = {'backend': args.backend, 'application': str(Path(args.application).resolve()),
              'checks': [], 'artifacts': str(root)}
    compositor = app = None
    states = []

    def mark(message):
        report['checks'].append(message)
        print(message, flush=True)

    def query(*argv):
        command = subprocess.run([str(Path(args.cli).resolve()), '--json', *map(str, argv)],
                                 env=env, capture_output=True, text=True, timeout=20)
        data = json.loads(command.stdout or command.stderr)
        if command.returncode or not data.get('ok'):
            raise RuntimeError(data)
        return data.get('data')

    def ready():
        try:
            return query('app', 'status')
        except (RuntimeError, json.JSONDecodeError):
            return False

    def groups_removed(items):
        return all(not Path('/sys/fs/cgroup' + item['cgroup']).exists() for item in items)

    def cleaned(items):
        return all(dead(item['pid']) and dead(item['descendant']) for item in items) and groups_removed(items)

    with (root / 'native.log').open('w') as log:
        try:
            compositor = subprocess.Popen([
                'weston', '--backend=headless', '--renderer=pixman', '--width=1280',
                '--height=900', '--socket=system-validation', '--idle-time=0',
                '--no-config', '--xwayland', f'--log={root}/weston.log'],
                env=env, stdout=log, stderr=log)
            wait_for(lambda: (runtime / 'system-validation').exists(), 'private Wayland display')
            if args.backend == 'xwayland':
                match = wait_for(lambda: re.search(r'xserver listening on display (:\d+)',
                                 (root / 'weston.log').read_text(), re.I), 'private XWayland display')
                env['DISPLAY'] = match[1]
            app = subprocess.Popen([str(Path(args.application).resolve()), '--foreground'],
                                   env=env, stdout=log, stderr=log, start_new_session=True)
            wait_for(ready, 'application startup')
            query('project', 'create', '--name', 'System fixture', '--root', root)
            project = query('project', 'list')['projects'][0]['id']
            for index in range(3):
                query('session', 'create', '--project', project, '--name', f'Fixture {index}')
                wait_for(lambda: len(list(root.glob('*.json'))) == index + 1, f'terminal {index}')
            paths = sorted(root.glob('*.json'))
            states = [json.loads(path.read_text()) for path in paths]
            children = set()
            for task in Path(f'/proc/{app.pid}/task').iterdir():
                try:
                    children.update((task / 'children').read_text().split())
                except FileNotFoundError:
                    pass
            watchdogs = [int(pid) for pid in children if b'--internal-terminal-watchdog'
                         in Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')]
            assert len(watchdogs) == 1, watchdogs
            watchdog = watchdogs[0]
            assert not any('/dev/pts/' in os.readlink(fd) for fd in Path(f'/proc/{watchdog}/fd').iterdir())
            ticks = lambda: sum(int(x) for x in Path(f'/proc/{watchdog}/stat').read_text().rsplit(')', 1)[1].split()[11:13])
            before = ticks()
            time.sleep(.5)
            assert ticks() == before, 'Watchdog consumed CPU with idle terminals'
            mark('Three real PTYs share one watchdog without PTY descriptors or idle CPU ticks')
            first_tab = int(paths[0].stem)
            query('tab', 'input', first_tab, '--text', 'literal $HOME; café 日本語', '--enter')
            wait_for(lambda: (root / f'{first_tab}.input').exists(), 'literal PTY input')
            assert (root / f'{first_tab}.input').read_text() == 'literal $HOME; café 日本語\n'
            mark('CLI transport delivers exact Unicode input and separate Enter')
            query('app', 'show')
            activated = subprocess.run([str(Path(args.application).resolve()), '--show'],
                                       env=env, capture_output=True, timeout=10)
            assert activated.returncode == 0, activated.stderr
            mark('CLI and repeated GUI launch activate the existing instance')
            query('tab', 'close', first_tab, '--confirm')
            wait_for(lambda: cleaned(states[:1]), 'independent terminal cleanup')
            assert all(not dead(item['pid']) and not dead(item['descendant']) for item in states[1:])
            mark('Independent terminal close cleans its cgroup and preserves siblings')
            query('app', 'quit', '--confirm')
            app.wait(timeout=10)
            wait_for(lambda: cleaned(states) and dead(watchdog), 'normal Quit cleanup')
            geometry = root / 'data/ongleet/window-state.json'
            assert json.loads(geometry.read_text())['width'] >= 640
            mark('Normal Quit saves geometry and cleans all terminals and watchdog')
            # Persistent definitions relaunch with new PIDs; an explicitly closed
            # tab stays closed. Compare fixture files, not just CLI status flags.
            for path in paths[1:]:
                path.unlink()
            app = subprocess.Popen([str(Path(args.application).resolve()), '--foreground'],
                                   env=env, stdout=log, stderr=log, start_new_session=True)
            wait_for(ready, 'application restart')
            wait_for(lambda: all(path.exists() for path in paths[1:]), 'restored terminals')
            restored = [json.loads(path.read_text()) for path in paths[1:]]
            assert all(item['pid'] != old['pid'] for item, old in zip(restored, states[1:]))
            assert query('app', 'status')['runningTerminals'] == 2
            assert json.loads(paths[0].read_text())['pid'] == states[0]['pid']
            states.extend(restored)
            restored_watchdogs = set()
            for task in Path(f'/proc/{app.pid}/task').iterdir():
                try:
                    restored_watchdogs.update((task / 'children').read_text().split())
                except FileNotFoundError:
                    pass
            restored_watchdogs = [int(pid) for pid in restored_watchdogs if b'--internal-terminal-watchdog'
                                  in Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')]
            assert len(restored_watchdogs) == 1
            if env.get('ONGLEET_DEBUG') == '1':
                diagnostic = root / 'data/ongleet/debug.log'
                assert 'native diagnostics installed' in diagnostic.read_text()
                assert '[native-input] enabled' in (root / 'native.log').read_text()
                mark('Native diagnostic events retain common formatting under ONGLEET_DEBUG')
            mark('Restart restores only eligible terminals as fresh processes')
            app.kill()
            app.wait(timeout=5)
            wait_for(lambda: cleaned(states) and all(dead(pid) for pid in restored_watchdogs), 'application crash cleanup')
            mark('SIGKILL cleans remaining leaders, detached descendants and cgroups')
        except Exception as error:
            report['error'] = str(error)
            raise
        finally:
            if app and app.poll() is None:
                app.kill()
                app.wait(timeout=5)
            if states:
                try:
                    wait_for(lambda: cleaned(states), 'final terminal cleanup', timeout=5)
                except RuntimeError:
                    pass
            if compositor:
                compositor.terminate()
                try:
                    compositor.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    compositor.kill()
                    compositor.wait(timeout=5)
            report['remainingLeaders'] = [item['pid'] for item in states if not dead(item['pid'])]
            report['remainingDescendants'] = [item['descendant'] for item in states if not dead(item['descendant'])]
            (root / 'report.json').write_text(json.dumps(report, indent=2))
            print('Report:', root / 'report.json', flush=True)
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application', required=True)
    parser.add_argument('--cli', required=True)
    parser.add_argument('--backend', choices=['wayland', 'xwayland'], required=True)
    raise SystemExit(run(parser.parse_args()))
