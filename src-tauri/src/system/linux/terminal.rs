use super::super::{
    Cancellation, Launch, TerminalControl, TerminalIo, TerminalReader, TerminalService,
    TerminalWait, TerminalWriter,
};
use super::containment::{Containment, Handshake};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
fn poll_handle(master: &dyn MasterPty) -> Result<std::fs::File, String> {
    let fd = master
        .as_raw_fd()
        .ok_or("PTY file descriptor unavailable")?;
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(duplicate) })
}

// A level-triggered eventfd stays readable after cancellation, waking every
// current or future waiter without a check/sleep race. Never drain this fd.
struct IoCancel {
    cancelled: AtomicBool,
    wake: std::fs::File,
}

impl IoCancel {
    fn new() -> Result<Self, String> {
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Self {
            cancelled: AtomicBool::new(false),
            wake: unsafe { std::fs::File::from_raw_fd(fd) },
        })
    }

    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            let value = 1u64;
            loop {
                let result =
                    unsafe { libc::write(self.wake.as_raw_fd(), (&value as *const u64).cast(), 8) };
                if result >= 0
                    || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                {
                    break;
                }
            }
        }
    }

    fn wait_io(&self, file: &std::fs::File, event: i16) -> Result<bool, String> {
        let mut fds = [
            libc::pollfd {
                fd: file.as_raw_fd(),
                events: event,
                revents: 0,
            },
            libc::pollfd {
                fd: self.wake.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Ok(false);
            }
            let result = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.to_string());
            }
            if fds[1].revents != 0 {
                return Ok(false);
            }
            if fds[0].revents & libc::POLLNVAL != 0 {
                return Err("PTY descriptor closed".into());
            }
            if fds[0].revents != 0 {
                return Ok(true);
            }
        }
    }
}

struct Reader {
    reader: Box<dyn Read + Send>,
    poll: std::fs::File,
    cancel: Arc<IoCancel>,
}
impl TerminalReader for Reader {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(bytes)
    }
    fn wait(&self) -> Result<bool, String> {
        self.cancel.wait_io(&self.poll, libc::POLLIN)
    }
}
struct Writer {
    writer: std::fs::File,
    poll: std::fs::File,
    cancel: Arc<IoCancel>,
}
impl TerminalWriter for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.writer.write(bytes)
    }
    fn wait(&self) -> Result<bool, String> {
        self.cancel.wait_io(&self.poll, libc::POLLOUT)
    }
}
impl Cancellation for IoCancel {
    fn cancel(&self) {
        IoCancel::cancel(self);
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
struct Control {
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    containment: Arc<Containment>,
}
impl TerminalControl for Control {
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if let Some(master) = self.master.lock().map_err(|e| e.to_string())?.as_ref() {
            master.resize(size(cols, rows)).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn stop(&self) -> Result<(), String> {
        self.containment.stop()
    }
    fn release(&self) {
        self.master.lock().unwrap().take();
    }
    #[cfg(test)]
    fn group_directory(&self) -> std::path::PathBuf {
        self.containment.directory().to_owned()
    }
    #[cfg(test)]
    fn released(&self) -> bool {
        self.master.lock().unwrap().is_none()
    }
}
struct Wait(Box<dyn portable_pty::Child + Send + Sync>);
impl TerminalWait for Wait {
    fn wait(mut self: Box<Self>) -> Result<u32, String> {
        self.0
            .wait()
            .map(|status| status.exit_code())
            .map_err(|e| e.to_string())
    }
}
impl TerminalService for super::Linux {
    fn spawn(&self, launch: Launch) -> Result<TerminalIo, String> {
        let executable = launch
            .executable
            .clone()
            .unwrap_or_else(|| super::super::SystemPaths::default_shell(self));
        let pair = native_pty_system()
            .openpty(size(launch.cols, launch.rows))
            .map_err(|e| e.to_string())?;
        // Prepare all fallible PTY handles before creating the child.
        let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let read_poll = poll_handle(pair.master.as_ref())?;
        let write_poll = poll_handle(pair.master.as_ref())?;
        // portable-pty's writer Drop injects newline + VEOF. Own a plain fd
        // instead: exit/close must not add bytes while the reader drains.
        let writer = write_poll.try_clone().map_err(|e| e.to_string())?;
        let cancel_input = Arc::new(IoCancel::new()?);
        let cancel_read = Arc::new(IoCancel::new()?);
        let containment = Arc::new(Containment::create()?);
        let handshake = Handshake::new()?;
        let mut cmd = CommandBuilder::new(super::containment::executable()?);
        cmd.arg(super::containment::HELPER_FLAG);
        cmd.arg(containment.directory());
        cmd.arg(handshake.socket());
        cmd.arg(executable);
        cmd.args(launch.args);
        cmd.cwd(launch.cwd);
        // Make the sibling CLI available inside app-launched processes without
        // changing the desktop environment or any shell configuration.
        let application = std::env::current_exe().map_err(|e| e.to_string())?;
        let directory = application
            .parent()
            .ok_or("Application directory is unavailable")?;
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(directory.to_path_buf()).chain(std::env::split_paths(&inherited)),
        )
        .map_err(|e| format!("Cannot construct terminal PATH: {e}"))?;
        cmd.env("PATH", path);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (key, value) in launch.env {
            cmd.env(key, value);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        if let Err(error) = handshake.wait(&containment) {
            let _ = containment.stop();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        drop(handshake);

        Ok(TerminalIo {
            reader: Box::new(Reader {
                reader,
                poll: read_poll,
                cancel: cancel_read.clone(),
            }),
            writer: Box::new(Writer {
                writer,
                poll: write_poll,
                cancel: cancel_input.clone(),
            }),
            cancel_input,
            cancel_read,
            control: Arc::new(Control {
                master: Mutex::new(Some(pair.master)),
                containment,
            }),
            wait: Box::new(Wait(child)),
        })
    }
}
fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};
    #[test]
    fn io_cancellation_wakes_all_waiters_and_remains_signalled() {
        use std::os::unix::net::UnixStream;
        let (socket, _peer) = UnixStream::pair().unwrap();
        let file = std::fs::File::from(std::os::fd::OwnedFd::from(socket));
        let cancel = Arc::new(IoCancel::new().unwrap());
        let (done, receive) = mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..2 {
            let file = file.try_clone().unwrap();
            let cancel = cancel.clone();
            let done = done.clone();
            workers.push(std::thread::spawn(move || {
                done.send(cancel.wait_io(&file, libc::POLLIN)).unwrap();
            }));
        }
        assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
        cancel.cancel();
        cancel.cancel();
        for _ in 0..2 {
            assert!(!receive
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap());
        }
        for worker in workers {
            worker.join().unwrap();
        }
        assert!(!cancel.wait_io(&file, libc::POLLIN).unwrap());
    }
}
