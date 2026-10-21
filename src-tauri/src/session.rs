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
            {
                let mut resources = reaper_resources.0.lock().unwrap();
                resources.status = Status::Exited {
                    code: exit.as_ref().map(|e| e.exit_code()).unwrap_or(1),
                };
                resources.input.take();
                resources.master.take();
            }
            {
                let mut flow = reaper_flow.0.lock().unwrap();
                flow.draining = true;
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
mod tests {
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
                Output::Size { seq, .. } | Output::Ready { seq } => {
                    session.acknowledge(seq).unwrap()
                }
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
            let Output::Data { seq, bytes } =
                new_receive.recv_timeout(Duration::from_secs(5)).unwrap()
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
        let Output::Data { seq, bytes } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
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
        let Output::Data { bytes, .. } = receive.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
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
                    "session::tests::crash_owner_fixture",
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
}
