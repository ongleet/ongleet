"""Check terminal detachment and immediate confirmed shutdown on a private X11 display.

Requires a bundled GUI build, matching CLI, Xvfb and WebKitWebDriver. No
native-validation frontend bridge or access to the user's clipboard is needed.
"""
import argparse
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import subprocess
import tempfile
import termios
import time

spec = importlib.util.spec_from_file_location("lifecycle", Path(__file__).with_name("check-lifecycle.py"))
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)
wait_for = lifecycle.wait_for


def alive(pid):
    try:
        return not Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].lstrip().startswith("Z")
    except FileNotFoundError:
        return False


def run(args):
    directory = Path(tempfile.mkdtemp(prefix="ongleet-window-close-"))
    report = {"artifacts": str(directory), "checks": []}
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"),
               XDG_CONFIG_HOME=str(directory / "config"), XDG_CACHE_HOME=str(directory / "cache"),
               GDK_BACKEND="x11")
    env.pop("TAURI_WEBVIEW_AUTOMATION", None)
    processes = []
    application_pids = []
    terminal_pid = None
    terminal_pids = []
    master = None

    def mark(message):
        report["checks"].append(message)
        print("PASS:", message, flush=True)

    def cli(*arguments, check=True):
        result = subprocess.run([args.cli, "--json", *arguments], env=env,
                                capture_output=True, text=True, timeout=35)
        if not check:
            return result
        assert result.returncode == 0, result.stderr
        return json.loads(result.stdout).get("data")

    with (directory / "native.log").open("w") as log:
        try:
            read_fd, write_fd = os.pipe()
            display = subprocess.Popen([args.xvfb, "-displayfd", str(write_fd), "-screen", "0", "1100x760x24", "-nolisten", "tcp"],
                                       pass_fds=(write_fd,), stdout=log, stderr=log)
            processes.append(display)
            os.close(write_fd)
            assert select.select([read_fd], [], [], 10)[0], "Private X server did not start"
            env["DISPLAY"] = ":" + os.read(read_fd, 64).decode().strip()
            os.close(read_fd)
            os.environ["DISPLAY"] = env["DISPLAY"]

            master, slave = pty.openpty()

            def controlling_terminal():
                os.setsid()
                fcntl.ioctl(0, termios.TIOCSCTTY, 0)

            shell = subprocess.Popen(["/bin/sh", "-c", '"$1"; result=$?; printf "SHELL_RETURNED:%s\\n" "$result"', "sh", args.application],
                                     stdin=slave, stdout=slave, stderr=slave, env=env,
                                     preexec_fn=controlling_terminal)
            processes.append(shell)
            os.close(slave)
            output = bytearray()
            deadline = time.monotonic() + 20
            while b"SHELL_RETURNED:0" not in output and time.monotonic() < deadline:
                if select.select([master], [], [], 0.2)[0]:
                    try:
                        output.extend(os.read(master, 4096))
                    except OSError:
                        break
            (directory / "launcher.log").write_bytes(output)
            assert b"SHELL_RETURNED:0" in output, output.decode(errors="replace")
            shell.wait(timeout=5)
            status = cli("app", "status")
            pid = status["pid"]
            application_pids.append(pid)
            assert alive(pid) and status["visible"]
            assert os.getsid(pid) == pid, "GUI did not create an independent session"
            assert Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[4] == "0", "GUI still owns a controlling terminal"
            os.close(master)
            master = None
            assert cli("app", "status")["pid"] == pid
            mark("Shell returns while detached GUI survives terminal closure")
            cli("app", "quit", "--confirm")
            wait_for(lambda: not alive(pid), "first GUI exit")

            with socket.socket() as peer:
                peer.bind(("127.0.0.1", 0))
                port = peer.getsockname()[1]
            automation_env = dict(env, TAURI_WEBVIEW_AUTOMATION="true")
            driver = subprocess.Popen([args.driver, "--host=127.0.0.1", f"--port={port}"], env=automation_env, stdout=log, stderr=log)
            processes.append(driver)
            client = lifecycle.WebDriver(f"http://127.0.0.1:{port}")
            wait_for(lambda: client.request("GET", "/status", session=False), "driver ready")
            response = client.request("POST", "/session", {"capabilities": {"alwaysMatch": {"webkitgtk:browserOptions": {"binary": args.application}}}}, session=False)
            client.session = response["sessionId"]
            wait_for(lambda: client.js("return !!document.querySelector('[aria-label=\"Create session\"]:not(:disabled)')"), "frontend ready")
            pid = cli("app", "status")["pid"]
            application_pids.append(pid)
            snapshot = client.invoke("create_project", {"name": "Close test", "rootDirectory": str(directory)})
            project = snapshot["projects"][-1]["id"]
            pid_file = directory / "terminal.pid"
            config = directory / "config/ongleet/projects" / str(project) / "actions.yaml"
            config.parent.mkdir(parents=True, exist_ok=True)
            child_file = directory / "child.pid"
            program = f"import os,time,subprocess,sys; child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(600)'],start_new_session=True);open({str(child_file)!r},'w').write(str(child.pid));open({str(pid_file)!r},'w').write(str(os.getpid()));time.sleep(600)"
            config.write_text("actions:\n  - id: sleeper\n    name: Sleeper\n    kind: terminal\n    config:\n      executable: /usr/bin/python3\n      args: " + json.dumps(["-c", program]) + "\n")
            client.invoke("create_session", {"name": "Close test", "directory": str(directory), "projectId": project})
            client.js("location.reload()")
            wait_for(pid_file.exists, "real terminal process")
            terminal_pid = int(pid_file.read_text())
            try:
                client.invoke("quit", {"confirmed": False})
            except RuntimeError as error:
                assert "confirmation" in str(error)
            else:
                raise AssertionError("Unconfirmed Quit accepted")
            assert cli("app", "status")["visible"] and alive(terminal_pid)
            mark("Unconfirmed Quit leaves window and terminal running")

            child_pid = int(child_file.read_text())
            terminal_pids.extend([terminal_pid, child_pid])
            wait_for(lambda: client.js('return !!document.querySelector(arguments[0])', '[aria-label="Close window"]:not(:disabled)'), 'cross enabled')
            warning = 'dialog[aria-label="Confirm action"][open]'
            def dismiss_confirmation(choice):
                wait_for(lambda: client.js('return !!document.querySelector(arguments[0])', warning), 'quit warning')
                text = client.js('return document.querySelector(arguments[0]).innerText', warning)
                client.click(f"//dialog[@aria-label='Confirm action']//button[normalize-space()='{choice}']", 'xpath')
                if choice == 'Cancel':
                    wait_for(lambda: client.js('return !document.querySelector(arguments[0])', warning), 'warning dismissed')
                return text

            client.click('[aria-label="Close window"]')
            cross_warning = dismiss_confirmation('Cancel')
            assert cli('app', 'status')['visible'] and alive(terminal_pid)
            mark('Cancelling the title-bar cross preserves window and terminal')
            client.click('summary[aria-label="Application menu"]')
            client.click('.app-menu-items > button:last-of-type')
            menu_warning = dismiss_confirmation('Cancel')
            assert menu_warning == cross_warning and alive(terminal_pid)
            mark('Cross and Quit use identical confirmation and cancellation behavior')
            lifecycle.close_window(pid)
            assert dismiss_confirmation('Cancel') == cross_warning
            assert alive(pid) and alive(terminal_pid)
            mark('Native window close uses the same cancellable Quit confirmation')
            client.click('[aria-label="Close window"]')
            dismiss_confirmation('Confirm')
            wait_for(lambda: not alive(pid), 'confirmed cross exits application', timeout=10)
            wait_for(lambda: not alive(terminal_pid) and not alive(child_pid), 'cross cleans up terminal and detached child', timeout=10)
            assert cli('app', 'status', check=False).returncode == 3
            mark('Confirmed cross exits immediately and cleans up its PTY without hiding or a timer')

            for route in ['menu', 'native']:
                try:
                    client.request('DELETE', '')
                except RuntimeError:
                    pass
                client.session = None
                response = client.request('POST', '/session', {'capabilities': {'alwaysMatch': {
                    'webkitgtk:browserOptions': {'binary': args.application}}}}, session=False)
                client.session = response['sessionId']
                wait_for(lambda: client.js('return !!document.querySelector(arguments[0])', '[aria-label="Close window"]:not(:disabled)'), 'restored application ready')
                pid = cli('app', 'status')['pid']
                application_pids.append(pid)
                wait_for(lambda: pid_file.exists() and int(pid_file.read_text()) != terminal_pid, 'fresh restored PTY')
                terminal_pid = int(pid_file.read_text())
                child_pid = int(child_file.read_text())
                terminal_pids.extend([terminal_pid, child_pid])
                if route == 'menu':
                    client.click('summary[aria-label="Application menu"]')
                    client.click('.app-menu-items > button:last-of-type')
                else:
                    lifecycle.close_window(pid)
                assert dismiss_confirmation('Confirm') == cross_warning
                wait_for(lambda: not alive(pid), f'confirmed {route} Quit exits application', timeout=10)
                wait_for(lambda: not alive(terminal_pid) and not alive(child_pid), f'{route} Quit cleans up terminal and child', timeout=10)
                mark(f'{route.capitalize()} Quit exits immediately and cleans up the fresh terminal and detached child')
        finally:
            if master is not None:
                os.close(master)
            try:
                cli("app", "quit", "--confirm", check=False)
            except subprocess.TimeoutExpired:
                pass
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            # Catch a detached GUI even when startup failed before its PID was reported.
            remaining = lifecycle.native.owned_processes(directory)
            report['forcedCleanupPids'] = [process['pid'] for process in remaining]
            for process in remaining:
                try:
                    os.kill(process['pid'], signal.SIGKILL)
                except ProcessLookupError:
                    pass
            if remaining:
                wait_for(lambda: not lifecycle.native.owned_processes(directory), 'failed-startup cleanup', timeout=5)
            report["survivingPids"] = [pid for pid in application_pids + terminal_pids if alive(pid)]
            (directory / "report.json").write_text(json.dumps(report, indent=2))
            print("Report:", directory / "report.json", flush=True)
    assert not report["survivingPids"], report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--application", required=True)
    parser.add_argument("--cli", required=True)
    parser.add_argument("--driver", required=True)
    parser.add_argument("--xvfb", required=True)
    run(parser.parse_args())
