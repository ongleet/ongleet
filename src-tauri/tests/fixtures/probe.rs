//! Rust process fixtures for native PTY tests; never use the user's shell config.
use std::io::{Read, Write};
pub fn run() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--probe") {
        return false;
    }
    match args.next().as_deref() {
        Some("argv") => {
            println!(
                "{}",
                serde_json::to_string(&args.collect::<Vec<_>>()).unwrap()
            );
        }
        Some("mark") => {
            std::fs::write(args.next().unwrap(), "executed").unwrap();
        }
        Some("flood") => {
            let bytes = [b'x'; 8192];
            loop {
                if std::io::stdout().write_all(&bytes).is_err() {
                    break;
                }
            }
        }
        Some("tree") => {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--probe", "hold"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            let mut ready = String::new();
            std::io::BufRead::read_line(
                &mut std::io::BufReader::new(child.stdout.take().unwrap()),
                &mut ready,
            )
            .unwrap();
            println!("TREE:{}:{}", std::process::id(), child.id());
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Some("owner") => {
            let services = crate::system::services();
            let mut resources = services
                .processes
                .spawn(
                    crate::system::ProcessLaunch {
                        executable: std::env::current_exe().unwrap(),
                        args: vec!["--probe".into(), "tree".into()],
                        ..Default::default()
                    },
                    crate::system::ProcessMode::Captured,
                )
                .unwrap();
            let mut ready = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                match resources.stdout.as_mut().unwrap().read(&mut buffer) {
                    Ok(0) => panic!("Tree exited before readiness"),
                    Ok(n) => {
                        ready.extend_from_slice(&buffer[..n]);
                        if ready.contains(&b'\n') {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5))
                    }
                    Err(e) => panic!("Readiness failed: {e}"),
                }
            }
            std::io::stdout().write_all(&ready).unwrap();
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Some("echo") => {
            let mut byte = [0];
            let mut input = Vec::new();
            while std::io::stdin().read_exact(&mut byte).is_ok() {
                input.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            std::io::stdout().write_all(b"PROBE:").unwrap();
            std::io::stdout().write_all(&input).unwrap();
            std::io::stdout().flush().unwrap();
        }
        Some("hold") => {
            println!("HOLD:{}", std::process::id());
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Some("escape") => {
            #[cfg(unix)]
            use std::os::unix::process::CommandExt;
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--probe", "hold"])
                .stdout(std::process::Stdio::piped());
            #[cfg(unix)]
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = command.spawn().unwrap();
            // Ensure setsid and readiness precede leader exit. Darwin may revoke
            // the old slave on leader death, so late writes would kill the probe.
            use std::io::BufRead;
            let mut ready = String::new();
            std::io::BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut ready)
                .unwrap();
            assert!(ready.starts_with("HOLD:"));
            println!("ESCAPED:{}", child.id());
        }
        _ => std::process::exit(2),
    }
    true
}
