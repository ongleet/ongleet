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
#[test]
fn owner_crash_closes_last_job_handle_and_kills_descendants() {
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
            .args(["--probe", "owner"])
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
