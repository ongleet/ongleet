//! Rust process fixtures for native PTY tests; never use the user's shell config.
use std::io::{Read, Write};
pub fn run() -> bool {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--probe") {
        return false;
    }
    match args.next().as_deref() {
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
            use std::os::unix::process::CommandExt;
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command.args(["--probe", "hold"]);
            unsafe {
                command.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let child = command.spawn().unwrap();
            println!("ESCAPED:{}", child.id());
        }
        _ => std::process::exit(2),
    }
    true
}
