use super::*;
use std::time::Instant;
use tauri::ipc::InvokeResponseBody;

fn launch(script: &str) -> Launch {
    Launch {
        executable: Some("/usr/bin/python3".into()),
        args: vec!["-u".into(), "-c".into(), script.into()],
        cwd: "/tmp".into(),
        env: BTreeMap::from([("PROTOTYPE_TEST".into(), "explicit-override".into())]),
        cols: 80,
        rows: 24,
    }
}

fn channel() -> (Channel<Output>, mpsc::Receiver<Output>) {
    let (send, receive) = mpsc::channel();
    let channel = Channel::new(move |body| {
        let InvokeResponseBody::Json(json) = body else {
            panic!("Expected JSON byte envelope")
        };
        let _ = send.send(serde_json::from_str(&json).unwrap());
        Ok(())
    });
    (channel, receive)
}

fn drain(session: &TerminalSession, receive: &mpsc::Receiver<Output>) -> Vec<u8> {
    let mut result = Vec::new();
    loop {
        match receive
            .recv_timeout(Duration::from_secs(5))
            .expect("PTY stalled")
        {
            Output::Data { seq, bytes } => {
                result.extend(bytes);
                session.acknowledge(seq).unwrap();
            }
            Output::Size { seq, .. } | Output::Ready { seq } => session.acknowledge(seq).unwrap(),
            Output::Exit { .. } => {}
            Output::Eof => return result,
            Output::Error { message } => panic!("PTY read failed: {message}"),
        }
    }
}

#[test]
fn pty_preserves_bytes_environment_and_exit_code() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(1, launch(
            "import os,sys; assert os.isatty(0); assert os.getcwd() == '/tmp'; assert os.environ['PROTOTYPE_TEST'] == 'explicit-override'; [os.write(1,bytes([b])) for b in 'café λ 🙂'.encode()]; sys.exit(7)"
        ), output).unwrap();
    let bytes = drain(&session, &receive);
    assert_eq!(bytes, "café λ 🙂".as_bytes());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Status::Exited { code } = session.status().unwrap() {
            assert_eq!(code, 7);
            break;
        }
        assert!(Instant::now() < deadline, "Child did not exit");
        std::thread::sleep(Duration::from_millis(10));
    }
    session.close().unwrap();
    session.close().unwrap(); // Resource cleanup is idempotent.
}

#[test]
fn reload_replays_pending_bytes_and_resizes_then_resumes_same_pty() {
    let (old_output, old_receive) = channel();
    let mut session = TerminalSession::spawn(20, launch(
            "import os,time; os.write(1,'before café 日本語 🙂'.encode()); input(); os.write(1,b'after-reload'); time.sleep(60)"
        ), old_output).unwrap();
    let Output::Data {
        seq: old_seq,
        bytes: original,
    } = old_receive.recv_timeout(Duration::from_secs(5)).unwrap()
    else {
        panic!("Expected initial output")
    };
    session.resize(103, 31).unwrap();
    let (new_output, new_receive) = channel();
    let attached = session.attach(new_output).unwrap();
    assert_eq!(
        (attached.id, attached.cols, attached.rows, attached.replay),
        (20, 80, 24, true)
    );
    assert!(
        session.acknowledge(old_seq).is_err(),
        "Old frontend released new attachment's pending data"
    );
    let mut replayed = Vec::new();
    let mut resized = false;
    loop {
        let event = new_receive.recv_timeout(Duration::from_secs(5)).unwrap();
        let (seq, ready) = match event {
            Output::Data { seq, bytes } => {
                replayed.extend(bytes);
                (seq, false)
            }
            Output::Size { seq, cols, rows } => {
                assert_eq!((cols, rows), (103, 31));
                resized = true;
                (seq, false)
            }
            Output::Ready { seq } => (seq, true),
            _ => panic!("Unexpected replay event"),
        };
        assert!(
            new_receive.recv_timeout(Duration::from_millis(25)).is_err(),
            "Replay bypassed ACK backpressure"
        );
        session.acknowledge(seq).unwrap();
        if ready {
            break;
        }
    }
    assert_eq!(replayed, original);
    assert!(resized);
    session
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let mut live = Vec::new();
    while !live
        .windows(b"after-reload".len())
        .any(|bytes| bytes == b"after-reload")
    {
        let Output::Data { seq, bytes } = new_receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected live output")
        };
        live.extend(bytes);
        session.acknowledge(seq).unwrap();
    }
    assert!(matches!(session.status().unwrap(), Status::Running));
    session.close().unwrap();
}

#[test]
fn reload_replays_acknowledged_output_after_process_exit() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(
        21,
        launch("import os; os.write(1,'finished café 🙂'.encode())"),
        output,
    )
    .unwrap();
    let original = drain(&session, &receive);
    let (output, receive) = channel();
    session.attach(output).unwrap();
    let mut replayed = Vec::new();
    let mut ready = false;
    loop {
        match receive.recv_timeout(Duration::from_secs(5)).unwrap() {
            Output::Data { seq, bytes } => {
                replayed.extend(bytes);
                session.acknowledge(seq).unwrap();
            }
            Output::Ready { seq } => {
                ready = true;
                session.acknowledge(seq).unwrap();
            }
            Output::Exit { code } => assert_eq!(code, 0),
            Output::Eof => break,
            _ => panic!("Unexpected replay event"),
        }
    }
    assert!(ready);
    assert_eq!(replayed, original);
    session.close().unwrap();
}

#[test]
fn replay_budget_discards_incomplete_history_instead_of_replaying_partial_vt() {
    let mut flow = Flow::default();
    for _ in 0..(REPLAY_LIMIT / CHUNK + 2) {
        flow.record(Record::Data(vec![b'x'; CHUNK]));
    }
    assert!(flow.incomplete);
    assert!(flow.journal.is_empty());
    assert_eq!(flow.journal_bytes, 0);
    flow.record(Record::Size(100, 30));
    assert!(flow.journal.is_empty());
}

#[test]
fn output_requires_ack_and_rejects_invalid_acknowledgements() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(2, launch(
            "import os; data=b'x'*1048576; offset=0\nwhile offset<len(data): offset+=os.write(1,data[offset:])"
        ), output).unwrap();
    let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("Expected output")
    };
    assert!(!bytes.is_empty() && bytes.len() <= CHUNK);
    assert!(
        receive.recv_timeout(Duration::from_millis(100)).is_err(),
        "Output bypassed backpressure"
    );
    assert!(session.acknowledge(seq + 1).is_err());
    session.acknowledge(seq).unwrap();
    assert!(session.acknowledge(0).is_err());
    let remaining = drain(&session, &receive);
    assert_eq!(bytes.len() + remaining.len(), 1048576);
    assert!(remaining.iter().all(|byte| *byte == b'x'));
    session.close().unwrap();
}

#[test]
fn resize_input_and_close_include_other_job_control_groups() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(3, launch(
            "import os,subprocess,time,sys\nchild=subprocess.Popen(['/usr/bin/python3','-c','import time; time.sleep(60)'],preexec_fn=os.setpgrp)\nprint('CHILD='+str(child.pid),flush=True)\nline=sys.stdin.readline()\nprint('SIZE='+str(os.get_terminal_size(0).columns)+'x'+str(os.get_terminal_size(0).lines),flush=True)\ntime.sleep(60)"
        ), output).unwrap();
    let mut collected = Vec::new();
    let child = loop {
        let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected child PID")
        };
        collected.extend(bytes);
        session.acknowledge(seq).unwrap();
        let text = String::from_utf8_lossy(&collected);
        if let Some(line) = text.lines().find(|line| line.starts_with("CHILD=")) {
            if text.ends_with('\n') {
                break line[6..].trim().parse::<i32>().unwrap();
            }
        }
    };
    session.resize(123, 37).unwrap();
    session
        .queue_input(b"probe\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    while !String::from_utf8_lossy(&collected).contains("SIZE=123x37") {
        let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("Expected size report")
        };
        collected.extend(bytes);
        session.acknowledge(seq).unwrap();
    }
    assert!(session.resize(0, 0).is_err());
    session.close().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let stat = std::fs::read_to_string(format!("/proc/{child}/stat"));
        if stat.as_ref().map_or(true, |s| {
            s.rsplit_once(')').unwrap().1.trim_start().starts_with('Z')
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Job-control child survived close"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn assert_dead(pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
        if stat.as_ref().map_or(true, |s| {
            s.rsplit_once(')').unwrap().1.trim_start().starts_with('Z')
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Detached descendant {pid} survived"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn detached_descendants_die_and_resources_release_without_status_polling() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(30, launch(
            "import os,time\nif os.fork()==0:\n os.setsid()\n if os.fork()==0:\n  print(os.getpid(),flush=True); time.sleep(60)\n os._exit(0)\ntime.sleep(.2); os._exit(9)"
        ), output).unwrap();
    let group = session.containment.directory().to_owned();
    let bytes = drain(&session, &receive);
    let pid = String::from_utf8(bytes).unwrap().trim().parse().unwrap();
    let (lock, notify) = &*session.resources;
    let resources = lock.lock().unwrap();
    let (resources, _) = notify
        .wait_timeout_while(resources, Duration::from_secs(5), |r| !r.completed)
        .unwrap();
    assert!(resources.completed);
    assert!(resources.master.is_none() && resources.input.is_none());
    assert!(matches!(resources.status, Status::Exited { code: 9 }));
    drop(resources);
    assert_dead(pid);
    assert!(!group.exists());
    session.close().unwrap();
}

#[test]
fn close_interrupts_unacknowledged_output_and_blocked_input() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(
        31,
        launch("import os,tty; tty.setraw(0)\nwhile True: os.write(1,b'x'*8192)"),
        output,
    )
    .unwrap();
    assert!(matches!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        Output::Data { .. }
    ));
    let mut pending = Vec::new();
    for _ in 0..100 {
        if let Ok(result) = session.queue_input(vec![b'x'; 8192]) {
            pending.push(result);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!pending.is_empty());
    let before = Instant::now();
    session.close().unwrap();
    assert!(before.elapsed() < Duration::from_secs(5));
    for result in pending {
        assert!(result.recv_timeout(Duration::from_secs(1)).is_ok());
    }
    assert!(session.resources.0.lock().unwrap().completed);
}

#[test]
fn crash_owner_fixture() {
    let Ok(path) = std::env::var("TERMINAL_TEST_CRASH_STATE") else {
        return;
    };
    let (output, receive) = channel();
    let session = TerminalSession::spawn(32, launch(
            "import os,subprocess,time; child=subprocess.Popen(['/usr/bin/python3','-c','import time; time.sleep(60)'],start_new_session=True); print(child.pid,flush=True); time.sleep(60)"
        ), output).unwrap();
    let Output::Data { bytes, .. } = receive.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("Missing PID")
    };
    std::fs::write(
        path,
        serde_json::to_vec(&(
            session.containment.directory(),
            String::from_utf8(bytes)
                .unwrap()
                .trim()
                .parse::<i32>()
                .unwrap(),
        ))
        .unwrap(),
    )
    .unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn application_death_watchdog_kills_detached_children() {
    struct Owner(std::process::Child);
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let path = std::env::temp_dir().join(format!("terminal-crash-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut owner = Owner(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "terminal_session::tests::crash_owner_fixture",
                "--nocapture",
            ])
            .env("TERMINAL_TEST_CRASH_STATE", &path)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let (group, pid): (std::path::PathBuf, i32) = loop {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(value) = serde_json::from_slice(&bytes) {
                break value;
            }
        }
        assert!(Instant::now() < deadline, "Fixture never launched");
        std::thread::sleep(Duration::from_millis(20));
    };
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    assert_dead(pid);
    let deadline = Instant::now() + Duration::from_secs(5);
    while group.exists() {
        assert!(
            Instant::now() < deadline,
            "Watchdog did not remove empty cgroup"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_directory_and_executable_fail_without_creating_directories() {
    let (output, _) = channel();
    let mut request = launch("pass");
    request.cwd = format!("/tmp/terminal-prototype-missing-{}", std::process::id());
    let directory = request.cwd.clone();
    assert!(TerminalSession::spawn(4, request, output).is_err());
    assert!(!std::path::Path::new(&directory).exists());
    let (output, _) = channel();
    let mut request = launch("pass");
    request.executable = Some("/nonexistent/terminal-prototype-executable".into());
    assert!(TerminalSession::spawn(5, request, output).is_err());
}

#[test]
fn terminal_path_includes_application_directory_and_explicit_override_wins() {
    let parent = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(
        901,
        launch("import os; print(os.environ['PATH'], end='')"),
        output,
    )
    .unwrap();
    let path = String::from_utf8(drain(&session, &receive)).unwrap();
    assert_eq!(std::env::split_paths(&path).next().unwrap(), parent);
    session.close().unwrap();

    let (output, receive) = channel();
    let mut command = launch("import os; print(os.environ['PATH'], end='')");
    command.env.insert("PATH".into(), "/usr/bin:/bin".into());
    let mut session = TerminalSession::spawn(902, command, output).unwrap();
    assert_eq!(drain(&session, &receive), b"/usr/bin:/bin");
    session.close().unwrap();
}

#[test]
fn exit_notification_bypasses_pending_output_and_survives_reattachment() {
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn(
        88,
        launch("import os; os.write(1, b'final output'); os.read(0, 1); os._exit(23)"),
        output,
    )
    .unwrap();
    // Deliberately do not ACK: process exit must not depend on painting or EOF.
    assert!(matches!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        Output::Data { .. }
    ));
    session
        .queue_input(b"\n".to_vec())
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert!(matches!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        Output::Exit { code: 23 }
    ));
    let (output, receive) = channel();
    session.attach(output).unwrap();
    assert!(matches!(
        receive.recv_timeout(Duration::from_secs(5)).unwrap(),
        Output::Exit { code: 23 }
    ));
    assert!(drain(&session, &receive).starts_with(b"final output"));
    session.close().unwrap();
}
