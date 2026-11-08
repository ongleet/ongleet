//! Worker contracts exercise real native PTYs through a Rust fixture process.
use super::*;
use std::{collections::BTreeMap, time::Instant};
fn channel() -> (Channel<Output>, mpsc::Receiver<Output>) {
    let (send, receive) = mpsc::channel();
    (
        Channel::new(move |body| {
            let tauri::ipc::InvokeResponseBody::Json(json) = body else {
                panic!("Expected JSON")
            };
            let _ = send.send(serde_json::from_str(&json).unwrap());
            Ok(())
        }),
        receive,
    )
}
#[test]
fn rust_fixture_roundtrip_preserves_unicode_and_enter() {
    let services = crate::system::services();
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!(
            "terminal-pty-helper{}",
            std::env::consts::EXE_SUFFIX
        ));
    let launch = Launch {
        executable: Some(helper.to_string_lossy().into_owned()),
        args: vec!["--probe".into(), "echo".into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        cols: 80,
        rows: 24,
    };
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn_with(
        services.terminal.as_ref(),
        services.paths.as_ref(),
        900,
        launch,
        output,
    )
    .unwrap();
    session
        .queue_text("héllo 世界".as_bytes().to_vec(), true)
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bytes = Vec::new();
    while Instant::now() < deadline {
        match receive.recv_timeout(Duration::from_secs(1)).unwrap() {
            Output::Data { seq, bytes: chunk } => {
                bytes.extend(chunk);
                session.acknowledge(seq).unwrap();
            }
            Output::Size { seq, .. } | Output::Ready { seq } => session.acknowledge(seq).unwrap(),
            Output::Eof => break,
            Output::Error { message } => panic!("{message}"),
            _ => (),
        }
    }
    assert!(String::from_utf8_lossy(&bytes).contains("PROBE:héllo 世界"));
    session.close().unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn escaped_descendant_cannot_hold_drain_open_and_detached_apps_survive_close() {
    let services = crate::system::services();
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!(
            "terminal-pty-helper{}",
            std::env::consts::EXE_SUFFIX
        ));
    let mut external = services
        .processes
        .spawn(
            crate::system::ProcessLaunch {
                executable: helper.clone(),
                args: vec!["--probe".into(), "hold".into()],
                ..Default::default()
            },
            crate::system::ProcessMode::Detached,
        )
        .unwrap();
    struct Kill(u32);
    impl Drop for Kill {
        fn drop(&mut self) {
            unsafe {
                libc::kill(self.0 as i32, libc::SIGKILL);
            }
        }
    }
    let external_guard = Kill(external.child.id());
    let launch = Launch {
        executable: Some(helper.to_string_lossy().into_owned()),
        args: vec!["--probe".into(), "escape".into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        cols: 80,
        rows: 24,
    };
    let (output, receive) = channel();
    let start = Instant::now();
    let mut session = TerminalSession::spawn_with(
        services.terminal.as_ref(),
        services.paths.as_ref(),
        901,
        launch,
        output,
    )
    .unwrap();
    let mut bytes = Vec::new();
    let mut escaped = None;
    loop {
        let output = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        match output {
            Output::Data { seq, bytes: chunk } => {
                bytes.extend(chunk);
                session.acknowledge(seq).unwrap();
                if escaped.is_none() {
                    let text = String::from_utf8_lossy(&bytes);
                    if let Some(pid) = text
                        .lines()
                        .find_map(|line| line.strip_prefix("ESCAPED:")?.trim().parse::<u32>().ok())
                    {
                        escaped = Some(Kill(pid));
                    }
                }
            }
            Output::Size { seq, .. } | Output::Ready { seq } => session.acknowledge(seq).unwrap(),
            Output::Error { message } => {
                assert!(message.contains("drain deadline"), "{message}");
            }
            Output::Eof => break,
            _ => (),
        }
    }
    assert!(escaped.is_some());
    // Darwin may revoke the PTY slave immediately. EOF and bounded truncation
    // are both valid; the escaped process must not hold closure indefinitely.
    assert_eq!(
        unsafe { libc::kill(escaped.as_ref().unwrap().0 as i32, 0) },
        0
    );
    assert!(start.elapsed() < Duration::from_secs(5));
    session.close().unwrap();
    assert!(external.child.try_wait().unwrap().is_none());
    drop(external_guard);
    external.child.wait().unwrap();
    drop(escaped);
}

#[test]
fn strict_containment_can_bound_a_reader_that_remains_open_after_exit() {
    // Simulate the native boundary retaining a readable stream after child exit.
    // The real PTY/process and all shared workers remain in the test path.
    struct Cancel(Mutex<Option<mpsc::Sender<()>>>);
    impl Cancellation for Cancel {
        fn cancel(&self) {
            self.0.lock().unwrap().take();
        }
        fn is_cancelled(&self) -> bool {
            self.0.lock().unwrap().is_none()
        }
    }
    struct Reader(mpsc::Receiver<()>);
    impl crate::system::TerminalReader for Reader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::WouldBlock.into())
        }
        fn wait(&self) -> Result<bool, String> {
            let _ = self.0.recv();
            Ok(false)
        }
    }
    struct HeldStream<'a>(&'a dyn TerminalService);
    impl TerminalService for HeldStream<'_> {
        fn post_exit_drain_timeout(&self) -> Option<Duration> {
            Some(Duration::from_secs(2))
        }
        fn spawn(&self, launch: Launch) -> Result<crate::system::TerminalIo, String> {
            let mut io = self.0.spawn(launch)?;
            let (send, receive) = mpsc::channel();
            io.reader = Box::new(Reader(receive));
            io.cancel_read = Arc::new(Cancel(Mutex::new(Some(send))));
            Ok(io)
        }
    }
    let services = crate::system::services();
    let helper = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(format!(
            "terminal-pty-helper{}",
            std::env::consts::EXE_SUFFIX
        ));
    let launch = Launch {
        executable: Some(helper.to_string_lossy().into_owned()),
        args: vec!["--probe".into(), "echo".into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        env: BTreeMap::new(),
        cols: 80,
        rows: 24,
    };
    let (output, receive) = channel();
    let mut session = TerminalSession::spawn_with(
        &HeldStream(services.terminal.as_ref()),
        services.paths.as_ref(),
        902,
        launch,
        output,
    )
    .unwrap();
    session
        .queue_text(b"exit".to_vec(), true)
        .unwrap()
        .recv_timeout(Duration::from_secs(2))
        .unwrap()
        .unwrap();
    let mut deadline_reported = false;
    loop {
        match receive.recv_timeout(Duration::from_secs(5)).unwrap() {
            Output::Error { message } => {
                assert!(message.contains("drain deadline"), "{message}");
                deadline_reported = true;
            }
            Output::Eof => break,
            _ => (),
        }
    }
    assert!(deadline_reported);
    session.close().unwrap();
}
