"""Check terminal detachment and two-minute shutdown on a private X11 display.

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
    directory = Path(tempfile.mkdtemp(prefix="devcenter-window-close-"))
    report = {"artifacts": str(directory), "checks": []}
    env = dict(os.environ, XDG_DATA_HOME=str(directory / "data"),
               XDG_CONFIG_HOME=str(directory / "config"), XDG_CACHE_HOME=str(directory / "cache"),
               GDK_BACKEND="x11")
    env.pop("TAURI_WEBVIEW_AUTOMATION", None)
    processes = []
    application_pids = []
    terminal_pid = None
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
            config = directory / "config/devcenter/projects" / str(project) / "actions.yaml"
            config.parent.mkdir(parents=True, exist_ok=True)
            config.write_text("actions:\n  - id: sleeper\n    name: Sleeper\n    kind: terminal\n    config:\n      executable: /usr/bin/python3\n      args: " + json.dumps(["-c", f"import os,time;open({str(pid_file)!r},'w').write(str(os.getpid()));time.sleep(600)"]) + "\n")
            client.invoke("create_session", {"name": "Close test", "directory": str(directory), "projectId": project})
            client.js("location.reload()")
            wait_for(pid_file.exists, "real terminal process")
            terminal_pid = int(pid_file.read_text())
            try:
                client.invoke("close_after_delay", {"confirmed": False})
            except RuntimeError as error:
                assert "confirmation" in str(error)
            else:
                raise AssertionError("Unconfirmed delayed close accepted")
            assert cli("app", "status")["visible"] and alive(terminal_pid)
            mark("Unconfirmed delayed shutdown leaves window and terminal running")

            def native_close(confirm):
                lifecycle.close_window(pid)
                wait_for(lambda: client.js("return !!document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), "close warning")
                client.click(f"//dialog[@aria-label='Confirm action']//button[normalize-space()='{confirm}']", "xpath")
                wait_for(lambda: client.js("return !document.querySelector('dialog[aria-label=\"Confirm action\"][open]')"), "close warning dismissed")

            native_close("Cancel")
            assert cli("app", "status")["visible"] and alive(terminal_pid)
            mark("Cancelling native close preserves window and terminal")
            native_close("Confirm")
            wait_for(lambda: not cli("app", "status")["visible"], "hidden window")
            assert alive(terminal_pid)
            cli("app", "show")
            wait_for(lambda: cli("app", "status")["visible"], "reopened window")
            print("Waiting past original two-minute deadline to verify cancellation...", flush=True)
            end = time.monotonic() + 123
            while time.monotonic() < end:
                assert alive(pid) and alive(terminal_pid)
                time.sleep(1)
            assert cli("app", "status")["pid"] == pid
            mark("Reopening cancels shutdown beyond original two-minute deadline")
            native_close("Confirm")
            wait_for(lambda: not cli("app", "status")["visible"], "second hidden window")
            started = time.monotonic()
            try:
                client.invoke("quit", {"confirmed": False})
            except RuntimeError as error:
                assert "confirmation" in str(error)
            else:
                raise AssertionError("Unconfirmed immediate Quit accepted")
            mark("Rejected immediate Quit preserves the scheduled shutdown")
            print("Waiting for confirmed two-minute shutdown...", flush=True)
            while time.monotonic() - started < 118:
                assert alive(pid) and alive(terminal_pid), "Shutdown happened before two minutes"
                time.sleep(1)
            wait_for(lambda: not alive(pid), "delayed application exit", timeout=15)
            wait_for(lambda: not alive(terminal_pid), "contained terminal cleanup")
            assert cli("app", "status", check=False).returncode == 3
            mark("Hidden app stays alive during delay, then exits and kills terminal at two minutes")
        finally:
            if master is not None:
                os.close(master)
            cli("app", "quit", "--confirm", check=False)
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            report["survivingPids"] = [pid for pid in application_pids + ([terminal_pid] if terminal_pid else []) if alive(pid)]
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
