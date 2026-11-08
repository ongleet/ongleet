use super::super::*;
use super::*;
use std::{
    io::{Read, Write},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
fn launch(mode: &str) -> Launch {
    Launch {
        executable: Some(
            Macos::new()
                .image
                .verified()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ),
        args: vec!["--probe".into(), mode.into()],
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        env: Default::default(),
        cols: 80,
        rows: 24,
    }
}
#[test]
fn independent_persistent_cancellation_wakes_blocked_read_and_write() {
    use std::os::unix::net::UnixStream;
    let (mut socket, _peer) = UnixStream::pair().unwrap();
    socket.set_nonblocking(true).unwrap();
    while socket.write(&[0; 8192]).is_ok() {}
    let file = std::fs::File::from(std::os::fd::OwnedFd::from(socket));
    let read = Arc::new(events::Cancel::new().unwrap());
    let write = Arc::new(events::Cancel::new().unwrap());
    let (send, receive) = mpsc::channel();
    for (name, cancel, filter) in [
        ("read", read.clone(), libc::EVFILT_READ),
        ("write", write.clone(), libc::EVFILT_WRITE),
    ] {
        let file = file.try_clone().unwrap();
        let send = send.clone();
        std::thread::spawn(move || {
            send.send((name, cancel.wait_io(&file, filter).unwrap()))
                .unwrap()
        });
    }
    assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
    read.cancel();
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(2)).unwrap(),
        ("read", false)
    );
    assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
    write.cancel();
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(2)).unwrap(),
        ("write", false)
    );
    assert!(!read.wait_io(&file, libc::EVFILT_READ).unwrap());
}
#[test]
fn private_transport_checks_real_peer_and_rejects_unexpected_paths() {
    let native = Macos::new();
    let path = paths::runtime()
        .unwrap()
        .join(format!("t-{}.sock", &native.nonce().unwrap()[..16]));
    let endpoint = ControlEndpoint(Arc::new(path.clone()));
    let listener = native.listen(&endpoint).unwrap();
    let mut client = native.connect(&endpoint).unwrap();
    client.verify_owner().unwrap();
    client.write_all(b"ping").unwrap();
    let mut peer = listener.accept().unwrap();
    peer.verify_owner().unwrap();
    let mut bytes = [0; 4];
    peer.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"ping");
    drop(listener);
    std::fs::write(&path, b"keep").unwrap();
    assert!(native.listen(&endpoint).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    std::fs::remove_file(path).unwrap();
}
#[test]
fn terminal_roundtrip_and_independent_close() {
    let native = Macos::new();
    let mut first = TerminalService::spawn(&native, launch("echo")).unwrap();
    let second = TerminalService::spawn(&native, launch("hold")).unwrap();
    first.writer.write("héllo\n".as_bytes()).unwrap();
    let mut bytes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let mut chunk = [0; 1024];
        match first.reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&chunk[..n]);
                if bytes
                    .windows("PROBE:héllo".len())
                    .any(|b| b == "PROBE:héllo".as_bytes())
                {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert!(String::from_utf8_lossy(&bytes).contains("PROBE:héllo"));
    assert_eq!(first.wait.wait().unwrap(), 0);
    first.control.stop().unwrap();
    second.control.resize(100, 30).unwrap();
    second.control.stop().unwrap();
    second.cancel_read.cancel();
    second.cancel_input.cancel();
    assert_ne!(second.wait.wait().unwrap(), 0);
}
#[test]
fn replacements_recreation_and_lock_release() {
    let native = Macos::new();
    let root = paths::runtime().unwrap().join(native.nonce().unwrap());
    native.private_directory(&root, false).unwrap();
    let path = root.join("file");
    std::fs::write(&path, "old").unwrap();
    let (send, receive) = mpsc::channel();
    let watch = native
        .watch(
            path.to_str().unwrap().into(),
            Box::new(move || {
                let _ = send.send(());
            }),
        )
        .unwrap();
    let temporary = root.join("new");
    std::fs::write(&temporary, "replacement").unwrap();
    std::fs::rename(&temporary, &path).unwrap();
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    watch.acknowledge();
    std::fs::remove_file(&path).unwrap();
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    watch.acknowledge();
    std::fs::write(&path, "recreated").unwrap();
    receive.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(watch);
    let lock = native.try_lock(&root.join("lock")).unwrap();
    assert!(native.try_lock(&root.join("lock")).is_err());
    drop(lock);
    drop(native.try_lock(&root.join("lock")).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

fn running(pid: i32) -> bool {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        ) == size
            && info.pbi_status != 5
    }
}
fn hold_pid(io: &mut TerminalIo) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut data = Vec::new();
    while Instant::now() < deadline {
        let mut bytes = [0; 1024];
        match io.reader.read(&mut bytes) {
            Ok(n) => {
                data.extend_from_slice(&bytes[..n]);
                if let Some(pid) = String::from_utf8_lossy(&data)
                    .lines()
                    .find_map(|line| line.strip_prefix("HOLD:")?.trim().parse().ok())
                {
                    return pid;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            other => panic!("Unexpected PTY read: {other:?}"),
        }
    }
    panic!("PTY fixture did not become ready")
}
#[test]
fn watchdog_owner_fixture() {
    let Some(path) = std::env::var_os("ONGLEET_TEST_DARWIN_FIXTURE") else {
        return;
    };
    let native = Macos::new();
    let mut first = TerminalService::spawn(&native, launch("hold")).unwrap();
    let one = hold_pid(&mut first);
    let watchdog = containment::watchdog_pid();
    let mut second = TerminalService::spawn(&native, launch("hold")).unwrap();
    let two = hold_pid(&mut second);
    assert_eq!(containment::watchdog_pid(), watchdog);
    std::fs::write(
        &path,
        serde_json::to_vec(&(vec![one, two], watchdog)).unwrap(),
    )
    .unwrap();
    if std::env::var_os("ONGLEET_TEST_PROTECTION_LOSS").is_some() {
        let mut byte = [0];
        std::io::stdin().read_exact(&mut byte).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !containment::protection_lost() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(containment::protection_lost());
        assert!(TerminalService::spawn(&native, launch("hold")).is_err());
        first.control.stop().unwrap();
        second.control.stop().unwrap();
    } else {
        loop {
            std::thread::park();
        }
    }
}
#[test]
fn application_crash_and_watchdog_loss_cleanup_without_unprotected_relaunch() {
    for loss in [false, true] {
        let native = Macos::new();
        let path = paths::runtime().unwrap().join(native.nonce().unwrap());
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "system::macos::tests::watchdog_owner_fixture",
                "--nocapture",
            ])
            .env("ONGLEET_TEST_DARWIN_FIXTURE", &path)
            .stdin(std::process::Stdio::piped());
        if loss {
            command.env("ONGLEET_TEST_PROTECTION_LOSS", "1");
        }
        struct FixtureChild(std::process::Child);
        impl Drop for FixtureChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut guard = FixtureChild(command.spawn().unwrap());
        let child = &mut guard.0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            assert!(child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(5));
        }
        let (pids, watchdog): (Vec<i32>, u32) =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if loss {
            assert_eq!(unsafe { libc::kill(watchdog as i32, libc::SIGKILL) }, 0);
            child.stdin.take().unwrap().write_all(&[1]).unwrap();
        } else {
            child.kill().unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|p| running(*p)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(pids.iter().all(|p| !running(*p)));
        let status = child.wait().unwrap();
        if loss {
            assert!(status.success());
        }
        std::fs::remove_file(path).unwrap();
    }
}
