use super::{super::*, Windows};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{mpsc, Arc},
    time::Duration,
};
fn root(label: &str) -> PathBuf {
    let native = Windows::new();
    let path =
        std::env::temp_dir().join(format!("ongleet-win-{label}-{}", native.nonce().unwrap()));
    native.private_directory(&path, true).unwrap();
    path
}
fn helper() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("terminal-pty-helper.exe")
}
fn launch(mode: &str) -> Launch {
    Launch {
        executable: Some(helper().to_string_lossy().into_owned()),
        args: vec!["--probe".into(), mode.into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        cols: 80,
        rows: 24,
    }
}
#[test]
fn conpty_unicode_resize_independence_and_reader_cancellation() {
    let native = Windows::new();
    let mut first = native.spawn_terminal(launch("echo"));
    let second = native.spawn_terminal(launch("hold"));
    first.control.resize(101, 37).unwrap();
    let (send, receive) = mpsc::channel();
    let mut reader = first.reader;
    let cancel = first.cancel_read.clone();
    let worker = std::thread::spawn(move || {
        let mut all = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => all.extend_from_slice(&buffer[..n]),
                Err(_) => break,
            }
        }
        send.send(all).unwrap();
    });
    let input = "héllo 世界\r".as_bytes();
    let mut position = 0;
    while position < input.len() {
        position += first.writer.write(&input[position..]).unwrap();
    }
    assert_eq!(first.wait.wait().unwrap(), 0);
    first.control.stop().unwrap();
    let output = match receive.recv_timeout(Duration::from_secs(2)) {
        Ok(output) => output,
        Err(_) => {
            cancel.cancel();
            receive.recv_timeout(Duration::from_secs(2)).unwrap()
        }
    };
    worker.join().unwrap();
    assert!(
        String::from_utf8_lossy(&output).contains("PROBE:héllo 世界"),
        "{:?}",
        String::from_utf8_lossy(&output)
    );
    second.control.resize(90, 30).unwrap();
    second.cancel_input.cancel();
    second.cancel_read.cancel();
    second.control.stop().unwrap();
}
impl Windows {
    fn spawn_terminal(&self, launch: Launch) -> TerminalIo {
        TerminalService::spawn(self, launch).unwrap()
    }
}
#[test]
fn blocked_conpty_io_is_independently_cancellable() {
    let native = Windows::new();
    let io = native.spawn_terminal(launch("hold"));
    let mut reader = io.reader;
    let mut writer = io.writer;
    let reader_cancel = io.cancel_read;
    let writer_cancel = io.cancel_input;
    let (sent, written) = mpsc::channel();
    let write_worker = std::thread::spawn(move || {
        let bytes = [b'x'; 8192];
        loop {
            if writer.write(&bytes).is_err() {
                break;
            }
        }
        sent.send(()).unwrap();
    });
    let (sent, read) = mpsc::channel();
    let read_worker = std::thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        while let Ok(n) = reader.read(&mut bytes) {
            if n == 0 {
                break;
            }
        }
        sent.send(()).unwrap();
    });
    assert!(written.recv_timeout(Duration::from_millis(100)).is_err());
    writer_cancel.cancel();
    written.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(!reader_cancel.is_cancelled());
    reader_cancel.cancel();
    read.recv_timeout(Duration::from_secs(3)).unwrap();
    io.control.stop().unwrap();
    write_worker.join().unwrap();
    read_worker.join().unwrap();
}
#[test]
fn named_pipe_half_close_identity_timeout_and_framing() {
    let native = Arc::new(Windows::new());
    let directory = root("pipe");
    let endpoint = native.control_endpoint(&directory).unwrap();
    let listener = native.listen(&endpoint).unwrap();
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().unwrap();
        connection.verify_owner().unwrap();
        let mut text = String::new();
        connection.read_to_string(&mut text).unwrap();
        assert_eq!(text, "héllo 世界");
        connection.write_all(b"response").unwrap();
        connection.finish_write().unwrap();
    });
    let mut connection = native.connect(&endpoint).unwrap();
    connection.verify_owner().unwrap();
    connection.write_all("héllo 世界".as_bytes()).unwrap();
    connection.finish_write().unwrap();
    let mut text = String::new();
    connection.read_to_string(&mut text).unwrap();
    assert_eq!(text, "response");
    server.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn private_files_lock_replacement_watch_and_recreation() {
    let native = Windows::new();
    let directory = root("files");
    let path = directory.join("file.txt");
    std::fs::write(&path, "old").unwrap();
    let lock = native.try_lock(&directory.join("lock")).unwrap();
    assert!(native.try_lock(&directory.join("lock")).is_err());
    drop(lock);
    drop(native.try_lock(&directory.join("lock")).unwrap());
    let (send, receive) = mpsc::channel();
    let watch = native
        .watch(
            path.to_string_lossy().into_owned(),
            Box::new(move || {
                let _ = send.send(());
            }),
        )
        .unwrap();
    let temporary = directory.join("replacement");
    std::fs::write(&temporary, "new").unwrap();
    native.atomic_replace(&temporary, &path).unwrap();
    receive.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    watch.acknowledge();
    std::fs::remove_file(&path).unwrap();
    receive.recv_timeout(Duration::from_secs(3)).unwrap();
    watch.acknowledge();
    std::fs::write(&path, "recreated").unwrap();
    receive.recv_timeout(Duration::from_secs(3)).unwrap();
    drop(watch);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn dpapi_persistence_replacement_deletion_and_corruption() {
    let native = Windows::new();
    let directory = root("dpapi");
    let path = directory.join("store.bin");
    let attributes = BTreeMap::from([("plugin".to_owned(), "private-test".to_owned())]);
    // Use the production transactional store with an isolated path. Public
    // SecretStore methods use the user's configured data directory instead.
    super::secrets::test_roundtrip(&native, &path, attributes);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn failed_assignment_never_executes_user_code() {
    let native = Windows::new();
    let directory = root("assignment");
    let marker = directory.join("must-not-exist");
    super::process::FAIL_ASSIGNMENT.with(|flag| flag.set(true));
    let result = ProcessService::spawn(
        &native,
        ProcessLaunch {
            executable: helper(),
            args: vec![
                "--probe".into(),
                "mark".into(),
                marker.to_string_lossy().into_owned(),
            ],
            ..Default::default()
        },
        ProcessMode::Captured,
    );
    super::process::FAIL_ASSIGNMENT.with(|flag| flag.set(false));
    assert!(result.err().unwrap().contains("before user execution"));
    assert!(!marker.exists());
    std::fs::remove_dir_all(directory).unwrap();
}
fn assert_owner_crash(mode: &str) {
    use std::io::BufRead;
    use windows_sys::Win32::System::Threading::*;
    struct Owner(std::process::Child);
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut owner = Owner(
        std::process::Command::new(helper())
            .args(["--probe", mode])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let output = owner.0.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = std::io::BufReader::new(output).read_line(&mut line);
        let _ = send.send((result, line));
    });
    let (result, line) = receive.recv_timeout(Duration::from_secs(10)).unwrap();
    result.unwrap();
    let pids: Vec<u32> = line
        .trim()
        .strip_prefix("TREE:")
        .unwrap()
        .split(':')
        .map(|value| value.parse().unwrap())
        .collect();
    assert_eq!(pids.len(), 2);
    let handles: Vec<_> = pids
        .into_iter()
        .map(|pid| {
            super::native::Handle::new(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) }).unwrap()
        })
        .collect();
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    for handle in handles {
        assert_eq!(
            unsafe { WaitForSingleObject(handle.0, 5000) },
            windows_sys::Win32::Foundation::WAIT_OBJECT_0
        );
    }
}
#[test]
fn executable_argument_quoting_preserves_unicode_and_metacharacters() {
    let native = Windows::new();
    let directory = root("quoting");
    let executable = directory.join("helper with space 日本語.exe");
    std::fs::copy(helper(), &executable).unwrap();
    let expected = vec![
        "",
        "a b",
        "it's quoted",
        "\"inside\"",
        "trailing\\",
        "x&y|z<>^%!",
        "日本語",
    ];
    let mut args = vec!["--probe".into(), "argv".into()];
    args.extend(expected.iter().map(|s| s.to_string()));
    let mut resources = ProcessService::spawn(
        &native,
        ProcessLaunch {
            executable,
            args,
            ..Default::default()
        },
        ProcessMode::Captured,
    )
    .unwrap();
    let mut bytes = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut buffer = [0u8; 4096];
    loop {
        match resources.stdout.as_mut().unwrap().read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(resources.child.wait().unwrap().exit_code, 0);
    let actual: Vec<String> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(actual, expected);
    drop(resources);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn command_shim_preserves_literal_arguments_in_a_contained_bootstrap() {
    let native = Windows::new();
    let directory = root("shim");
    let shim = directory.join("command with spaces.cmd");
    // A conventional Windows launcher forwards its original argument tail.
    std::fs::write(
        &shim,
        format!("@\"{}\" --probe argv %*\r\n", helper().display()),
    )
    .unwrap();
    let expected = vec![
        "",
        "a b",
        "it's quoted",
        "trailing\\",
        "x&y|z<>^%!",
        "日本語",
    ];
    let mut resources = ProcessService::spawn(
        &native,
        ProcessLaunch {
            executable: shim,
            args: expected.iter().map(|arg| arg.to_string()).collect(),
            ..Default::default()
        },
        ProcessMode::Captured,
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match resources.stdout.as_mut().unwrap().read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(resources.child.wait().unwrap().exit_code, 0);
    let actual: Vec<String> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(actual, expected);
    drop(resources);
    std::fs::remove_dir_all(directory).unwrap();
}

fn block_on<T>(future: SystemFuture<'_, T>) -> Result<T, String> {
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(Wake(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = future;
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::park(),
        }
    }
}
#[test]
fn dpapi_public_contract_isolates_attributes_and_enforces_payload_limits() {
    let directory = root("secret-contract");
    let mut native = Windows::new();
    let path = directory.join("private.bin");
    native.secret_path = Some(path.clone());
    let first = BTreeMap::from([
        ("plugin".into(), "a".into()),
        ("token".into(), "one".into()),
    ]);
    let second = BTreeMap::from([
        ("plugin".into(), "b".into()),
        ("token".into(), "two".into()),
    ]);
    let payload = vec![b'x'; 64 * 1024];
    block_on(native.ensure_available()).unwrap();
    block_on(native.create(&first, &payload, true)).unwrap();
    block_on(native.create(&second, b"other", true)).unwrap();
    assert!(block_on(native.create(&first, &vec![0; 64 * 1024 + 1], true)).is_err());
    let search = block_on(native.search(&BTreeMap::from([("plugin".into(), "a".into())]))).unwrap();
    assert_eq!(search.records.len(), 1);
    assert_eq!(block_on(native.read(&search.records[0])).unwrap(), payload);
    block_on(native.create(&first, b"replacement", true)).unwrap();
    assert!(block_on(native.read(&search.records[0])).is_err());
    let search = block_on(native.search(&first)).unwrap();
    assert_eq!(search.records.len(), 1);
    assert_eq!(
        block_on(native.read(&search.records[0])).unwrap(),
        b"replacement"
    );
    block_on(native.remove(&search.records[0])).unwrap();
    assert!(block_on(native.search(&first)).unwrap().records.is_empty());
    assert_eq!(block_on(native.search(&second)).unwrap().records.len(), 1);
    std::fs::write(&path, b"invalid encrypted store").unwrap();
    assert!(block_on(native.ensure_available()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"invalid encrypted store");
    let invalid = directory.join("directory-instead-of-store");
    std::fs::create_dir(&invalid).unwrap();
    native.secret_path = Some(invalid);
    assert!(block_on(native.ensure_available()).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn named_pipe_read_deadline_and_missing_peer_are_bounded() {
    let native = Windows::new();
    let directory = root("pipe-timeout");
    let endpoint = native.control_endpoint(&directory).unwrap();
    let listener = native.listen(&endpoint).unwrap();
    let (ready, receive) = mpsc::channel();
    let (done, finish) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let connection = listener.accept().unwrap();
        ready.send(()).unwrap();
        finish.recv_timeout(Duration::from_secs(10)).unwrap();
        drop(connection);
    });
    let mut connection = native.connect(&endpoint).unwrap();
    receive.recv_timeout(Duration::from_secs(3)).unwrap();
    connection.read_timeout(Duration::from_millis(80)).unwrap();
    let start = std::time::Instant::now();
    let error = connection.read(&mut [0u8; 1]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(2));
    done.send(()).unwrap();
    worker.join().unwrap();
    assert!(native.connect(&endpoint).is_err());
    assert!(super::native::verify_pid(0).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn oversized_native_pipe_frames_are_rejected_before_payload_allocation() {
    use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*};
    let native = Windows::new();
    let directory = root("pipe-frame");
    let endpoint = native.control_endpoint(&directory).unwrap();
    let listener = native.listen(&endpoint).unwrap();
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut connection = listener.accept().unwrap();
        let error = connection.read(&mut [0u8; 1]).unwrap_err();
        send.send(error.kind()).unwrap();
    });
    let name = endpoint.0.downcast_ref::<String>().unwrap();
    let handle = super::native::Handle::new(unsafe {
        CreateFileW(
            super::native::wide(name).unwrap().as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    })
    .unwrap();
    let io = super::native::PipeIo {
        handle: Arc::new(handle),
        cancel: super::native::Cancel::new().unwrap(),
    };
    let mut header = (1024 * 1024 + 1u32).to_le_bytes();
    assert_eq!(io.transfer(header.as_mut_ptr(), 4, true, 2000).unwrap(), 4);
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(3)).unwrap(),
        std::io::ErrorKind::InvalidData
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn owner_crash_closes_terminal_and_captured_jobs_and_kills_descendants() {
    for mode in ["owner", "terminal-owner"] {
        assert_owner_crash(mode);
    }
}

#[test]
fn detached_actions_survive_terminal_closure_or_report_host_job_restriction() {
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    let native = Windows::new();
    let detached = ProcessService::spawn(
        &native,
        ProcessLaunch {
            executable: helper(),
            args: vec!["--probe".into(), "hold".into()],
            ..Default::default()
        },
        ProcessMode::Detached,
    );
    let resources = match detached {
        Ok(resources) => resources,
        Err(error) => {
            let mut in_job = 0;
            super::native::check(unsafe {
                windows_sys::Win32::System::JobObjects::IsProcessInJob(
                    GetCurrentProcess(),
                    std::ptr::null_mut(),
                    &mut in_job,
                )
            })
            .unwrap();
            assert!(
                in_job != 0 && error.contains("enclosing host Job") && error.contains("os error 5"),
                "{error}"
            );
            eprintln!("Host Job forbids breakaway; detached launch correctly rejected");
            return;
        }
    };
    let handle = super::native::Handle::new(unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
            0,
            resources.child.id(),
        )
    })
    .unwrap();
    struct Stop(super::native::Handle);
    impl Drop for Stop {
        fn drop(&mut self) {
            unsafe {
                TerminateProcess(self.0 .0, 1);
                WaitForSingleObject(self.0 .0, 5000);
            }
        }
    }
    let child = Stop(handle);
    let terminal = native.spawn_terminal(launch("hold"));
    terminal.cancel_read.cancel();
    terminal.cancel_input.cancel();
    terminal.control.stop().unwrap();
    drop(terminal);
    drop(resources);
    drop(native);
    assert_eq!(unsafe { WaitForSingleObject(child.0 .0, 0) }, WAIT_TIMEOUT);
}

#[test]
fn environment_layers_treat_names_case_insensitively_and_preserve_reserved_context() {
    let native = Windows::new();
    let mut env = BTreeMap::from([
        ("SDK_CASE".into(), "project".into()),
        ("ongleet_session_id".into(), "untrusted".into()),
    ]);
    native.merge_environment(
        &mut env,
        BTreeMap::from([("sdk_case".into(), "action".into())]),
    );
    native.merge_environment(
        &mut env,
        BTreeMap::from([("ONGLEET_SESSION_ID".into(), "123".into())]),
    );
    let mut resources = ProcessService::spawn(
        &native,
        ProcessLaunch {
            executable: helper(),
            args: vec![
                "--probe".into(),
                "environment".into(),
                "SDK_CASE".into(),
                "sdk_case".into(),
                "ONGLEET_SESSION_ID".into(),
                "ongleet_session_id".into(),
            ],
            env,
            ..Default::default()
        },
        ProcessMode::Captured,
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        match resources.stdout.as_mut().unwrap().read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(resources.child.wait().unwrap().exit_code, 0);
    let values: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(values["SDK_CASE"], "action");
    assert_eq!(values["sdk_case"], "action");
    assert_eq!(values["ONGLEET_SESSION_ID"], "123");
    assert_eq!(values["ongleet_session_id"], "123");
}
