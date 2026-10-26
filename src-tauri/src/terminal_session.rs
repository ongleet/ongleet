use crate::containment::{Containment, Handshake};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    time::Duration,
};
use tauri::ipc::Channel;

const CHUNK: usize = 16 * 1024;
const REPLAY_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Launch {
    pub executable: Option<String>,
    pub args: Vec<String>,
    pub cwd: String,
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Output {
    Data { seq: u64, bytes: Vec<u8> },
    Size { seq: u64, cols: u16, rows: u16 },
    Ready { seq: u64 },
    Exit { code: u32 },
    Eof,
    Error { message: String },
}

#[derive(Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Status {
    Running,
    Exited { code: u32 },
}

#[derive(Clone)]
enum Record {
    Data(Vec<u8>),
    Size(u16, u16),
    Ready,
}

impl Record {
    fn cost(&self) -> usize {
        32 + match self {
            Self::Data(bytes) => bytes.len(),
            _ => 0,
        }
    }
}

#[derive(Default)]
struct Flow {
    sent: u64,
    acked: u64,
    closed: bool,
    output: Option<Channel<Output>>,
    journal: Vec<Record>,
    journal_bytes: usize,
    incomplete: bool,
    pending: VecDeque<Record>,
    ended: bool,
    draining: bool,
    exit_code: Option<u32>,
}

impl Flow {
    fn record(&mut self, record: Record) {
        if self.incomplete {
            return;
        }
        if self.journal_bytes + record.cost() > REPLAY_LIMIT {
            self.journal.clear();
            self.journal_bytes = 0;
            self.incomplete = true;
        } else {
            self.journal_bytes += record.cost();
            self.journal.push(record);
        }
    }

    // Only one envelope may await ACK, including during historical replay.
    fn dispatch(&mut self) {
        if self.closed || self.sent != self.acked {
            return;
        }
        if let Some(record) = self.pending.pop_front() {
            self.sent += 1;
            let seq = self.sent;
            let event = match record {
                Record::Data(bytes) => Output::Data { seq, bytes },
                Record::Size(cols, rows) => Output::Size { seq, cols, rows },
                Record::Ready => Output::Ready { seq },
            };
            if let Some(output) = &self.output {
                let _ = output.send(event);
            }
            // A disconnected frontend leaves this envelope pending until attach.
        } else if self.ended {
            if let Some(output) = &self.output {
                let _ = output.send(Output::Eof);
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: u64,
    pub cols: u16,
    pub rows: u16,
    pub replay: bool,
}

struct Input {
    bytes: Vec<u8>,
    done: mpsc::Sender<Result<(), String>>,
}

// This boundary owns native resources; React only owns an emulator and attachment.
// Replay data is bounded and in-memory only; no terminal daemon is involved.
struct Resources {
    master: Option<Box<dyn MasterPty + Send>>,
    input: Option<mpsc::SyncSender<Input>>,
    status: Status,
    completed: bool,
    cleanup_error: Option<String>,
}

pub struct TerminalSession {
    pub id: u64,
    pub previous_history: String,
    initial_cols: u16,
    initial_rows: u16,
    resources: Arc<(Mutex<Resources>, Condvar)>,
    containment: Arc<Containment>,
    cancel_input: Arc<AtomicBool>,
    flow: Arc<(Mutex<Flow>, Condvar)>,
    closed: bool,
}

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

fn wait_io(file: &std::fs::File, event: i16) {
    let mut fd = libc::pollfd {
        fd: file.as_raw_fd(),
        events: event,
        revents: 0,
    };
    unsafe {
        libc::poll(&mut fd, 1, 50);
    }
}

impl TerminalSession {
    pub fn spawn(id: u64, launch: Launch, output: Channel<Output>) -> Result<Self, String> {
        if !std::path::Path::new(&launch.cwd).is_dir() {
            return Err("Working directory must be an existing directory".into());
        }
        if launch.cols < 2 || launch.rows == 0 || launch.cols > 1000 || launch.rows > 1000 {
            return Err("Invalid terminal dimensions".into());
        }
        let executable = launch
            .executable
            .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
        if executable.is_empty() {
            return Err("Executable cannot be empty".into());
        }
        let pair = native_pty_system()
            .openpty(size(launch.cols, launch.rows))
            .map_err(|e| e.to_string())?;
        // Prepare all fallible PTY handles before creating the child.
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let read_poll = poll_handle(pair.master.as_ref())?;
        let write_poll = poll_handle(pair.master.as_ref())?;
        let containment = Arc::new(Containment::create()?);
        let handshake = Handshake::new()?;
        let mut cmd = CommandBuilder::new(crate::containment::executable()?);
        cmd.arg(crate::containment::HELPER_FLAG);
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
        if let Err(error) = handshake.wait() {
            let _ = containment.stop();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        drop(handshake);
        let flow = Arc::new((
            Mutex::new(Flow {
                output: Some(output),
                ..Default::default()
            }),
            Condvar::new(),
        ));
        let cancel_input = Arc::new(AtomicBool::new(false));
        let writer_cancel = cancel_input.clone();
        let (input, pending_input) = mpsc::sync_channel::<Input>(1);
        let writer_thread = std::thread::spawn(move || {
            while let Ok(request) = pending_input.recv() {
                let result = (|| {
                    let mut bytes = request.bytes.as_slice();
                    while !bytes.is_empty() {
                        if writer_cancel.load(Ordering::Acquire) {
                            return Err("Terminal has exited".into());
                        }
                        match writer.write(bytes) {
                            Ok(0) => return Err("PTY input closed".into()),
                            Ok(n) => bytes = &bytes[n..],
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                wait_io(&write_poll, libc::POLLOUT)
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                            Err(error) => return Err(error.to_string()),
                        }
                    }
                    Ok(())
                })();
                let _ = request.done.send(result);
            }
        });
        let reader_flow = flow.clone();
        let reader_thread = std::thread::spawn(move || {
            let mut drained = 0usize;
            let (lock, changed) = &*reader_flow;
            let mut bytes = [0u8; CHUNK];
            loop {
                // Reading waits for frontend consumption, including replay.
                let mut state = lock.lock().unwrap();
                while !state.closed
                    && !state.draining
                    && (state.acked != state.sent || !state.pending.is_empty())
                {
                    state = changed
                        .wait_timeout(state, Duration::from_secs(1))
                        .unwrap()
                        .0;
                }
                if state.closed {
                    break;
                }
                drop(state);
                match reader.read(&mut bytes) {
                    Ok(0) => {
                        let mut state = lock.lock().unwrap();
                        state.ended = true;
                        state.dispatch();
                        break;
                    }
                    Ok(n) => {
                        let mut state = lock.lock().unwrap();
                        if state.closed {
                            break;
                        }
                        if state.draining {
                            drained += n;
                            if drained > 1024 * 1024 {
                                if let Some(output) = &state.output {
                                    let _ = output.send(Output::Error { message: "Post-exit output exceeded the 1 MiB drain limit; remaining output was discarded".into() });
                                }
                                state.ended = true;
                                state.dispatch();
                                break;
                            }
                        }
                        let record = Record::Data(bytes[..n].to_vec());
                        state.record(record.clone());
                        state.pending.push_back(record);
                        state.dispatch();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        wait_io(&read_poll, libc::POLLIN)
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        let mut state = lock.lock().unwrap();
                        state.ended = true;
                        if let Some(output) = &state.output {
                            let _ = output.send(Output::Error {
                                message: e.to_string(),
                            });
                        }
                        state.dispatch();
                        break;
                    }
                }
            }
        });
        let resources = Arc::new((
            Mutex::new(Resources {
                master: Some(pair.master),
                input: Some(input),
                status: Status::Running,
                completed: false,
                cleanup_error: None,
            }),
            Condvar::new(),
        ));
        let reaper_resources = resources.clone();
        let reaper_containment = containment.clone();
        let reaper_flow = flow.clone();
        let reaper_cancel = cancel_input.clone();
        std::thread::spawn(move || {
            let exit = loop {
                match child.try_wait() {
                    Ok(Some(exit)) => break Ok(exit),
                    Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                    Err(error) => {
                        let _ = reaper_containment.stop();
                        break child.wait().map_err(|_| error.to_string());
                    }
                }
            };
            let cleanup = reaper_containment.stop();
            reaper_cancel.store(true, Ordering::Release);
            let code = exit.as_ref().map(|e| e.exit_code()).unwrap_or(1);
            {
                let mut resources = reaper_resources.0.lock().unwrap();
                resources.status = Status::Exited { code };
                resources.input.take();
                resources.master.take();
            }
            {
                let mut flow = reaper_flow.0.lock().unwrap();
                flow.draining = true;
                flow.exit_code = Some(code);
                if let Some(output) = &flow.output {
                    // A control notification bypasses data ACKs: exit status must
                    // arrive even when a frontend is still consuming output.
                    let _ = output.send(Output::Exit { code });
                }
                if let Err(error) = &cleanup {
                    if let Some(output) = &flow.output {
                        let _ = output.send(Output::Error {
                            message: format!("Terminal process cleanup failed: {error}"),
                        });
                    }
                    flow.closed = true;
                }
                reaper_flow.1.notify_all();
            }
            let _ = writer_thread.join();
            let _ = reader_thread.join();
            let mut resources = reaper_resources.0.lock().unwrap();
            resources.cleanup_error = cleanup.err().or_else(|| exit.err());
            resources.completed = true;
            reaper_resources.1.notify_all();
        });
        Ok(Self {
            id,
            previous_history: String::new(),
            initial_cols: launch.cols,
            initial_rows: launch.rows,
            resources,
            containment,
            cancel_input,
            flow,
            closed: false,
        })
    }

    pub fn attach(&self, output: Channel<Output>) -> Result<Attachment, String> {
        let (lock, changed) = &*self.flow;
        let mut flow = lock.lock().map_err(|e| e.to_string())?;
        if flow.closed {
            return Err("Terminal is stopped".into());
        }
        flow.output = Some(output);
        // Sequence numbers never reset: an old frontend cannot ACK replay data.
        flow.acked = flow.sent;
        flow.pending = flow.journal.clone().into();
        if flow.incomplete {
            let _ = flow.output.as_ref().unwrap().send(Output::Error {
                message: "Previous terminal screen unavailable: the 4 MiB reload replay limit was exceeded. The terminal session is retained; only new output is shown.".into(),
            });
        }
        if let Some(code) = flow.exit_code {
            let _ = flow.output.as_ref().unwrap().send(Output::Exit { code });
        }
        flow.pending.push_back(Record::Ready);
        flow.dispatch();
        changed.notify_all();
        Ok(Attachment {
            id: self.id,
            cols: self.initial_cols,
            rows: self.initial_rows,
            replay: true,
        })
    }

    pub fn acknowledge(&self, seq: u64) -> Result<(), String> {
        let (lock, changed) = &*self.flow;
        let mut flow = lock.lock().unwrap();
        if seq != flow.sent || seq <= flow.acked {
            return Err("Unexpected output acknowledgement".into());
        }
        flow.acked = seq;
        flow.dispatch();
        changed.notify_all();
        Ok(())
    }

    pub fn queue_input(
        &self,
        bytes: Vec<u8>,
    ) -> Result<mpsc::Receiver<Result<(), String>>, String> {
        if bytes.len() > 8192 {
            return Err("Input chunk exceeds 8 KiB".into());
        }
        let (done, result) = mpsc::channel();
        self.resources
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .input
            .as_ref()
            .ok_or("Terminal is stopped")?
            .try_send(Input { bytes, done })
            .map_err(|e| e.to_string())?;
        Ok(result)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if cols < 2 || rows == 0 || cols > 1000 || rows > 1000 {
            return Err("Terminal dimensions must be 2..1000 columns and 1..1000 rows".into());
        }
        if let Some(master) = &self.resources.0.lock().map_err(|e| e.to_string())?.master {
            master.resize(size(cols, rows)).map_err(|e| e.to_string())?;
        }
        self.flow
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .record(Record::Size(cols, rows));
        Ok(())
    }

    pub fn status(&mut self) -> Result<Status, String> {
        Ok(self
            .resources
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .status
            .clone())
    }

    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        self.containment.stop()?;
        self.cancel_input.store(true, Ordering::Release);
        self.flow.0.lock().unwrap().closed = true;
        self.flow.1.notify_all();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut resources = self.resources.0.lock().map_err(|e| e.to_string())?;
        while !resources.completed {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("Timed out releasing terminal workers".into());
            }
            resources = self
                .resources
                .1
                .wait_timeout(resources, remaining)
                .map_err(|e| e.to_string())?
                .0;
        }
        if let Some(error) = &resources.cleanup_error {
            return Err(error.clone());
        }
        self.closed = true;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.close();
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
mod tests;
