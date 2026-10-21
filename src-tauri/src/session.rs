use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    sync::{mpsc, Arc, Condvar, Mutex},
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
pub struct TerminalSession {
    pub id: u64,
    pid: i32,
    initial_cols: u16,
    initial_rows: u16,
    master: Option<Box<dyn MasterPty + Send>>,
    input: Option<mpsc::SyncSender<Input>>,
    child: Box<dyn Child + Send + Sync>,
    status: Status,
    flow: Arc<(Mutex<Flow>, Condvar)>,
    closed: bool,
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
        let mut cmd = CommandBuilder::new(executable);
        cmd.args(launch.args);
        cmd.cwd(launch.cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (key, value) in launch.env {
            cmd.env(key, value);
        }
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        let pid = child.process_id().expect("Unix child has a PID") as i32;
        drop(pair.slave);
        let flow = Arc::new((
            Mutex::new(Flow {
                output: Some(output),
                ..Default::default()
            }),
            Condvar::new(),
        ));
        let (input, pending_input) = mpsc::sync_channel::<Input>(1);
        std::thread::spawn(move || {
            while let Ok(request) = pending_input.recv() {
                let result = writer.write_all(&request.bytes).map_err(|e| e.to_string());
                let failed = result.is_err();
                let _ = request.done.send(result);
                if failed {
                    break;
                }
            }
        });
        let reader_flow = flow.clone();
        std::thread::spawn(move || {
            let (lock, changed) = &*reader_flow;
            let mut bytes = [0u8; CHUNK];
            loop {
                // Reading waits for frontend consumption, including replay.
                let mut state = lock.lock().unwrap();
                while !state.closed && (state.acked != state.sent || !state.pending.is_empty()) {
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
                        let record = Record::Data(bytes[..n].to_vec());
                        state.record(record.clone());
                        state.pending.push_back(record);
                        state.dispatch();
                    }
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
        Ok(Self {
            id,
            pid,
            initial_cols: launch.cols,
            initial_rows: launch.rows,
            master: Some(pair.master),
            input: Some(input),
            child,
            status: Status::Running,
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
                message: "Previous terminal screen unavailable: the 4 MiB reload replay limit was exceeded. The process is still running; only new output is shown.".into(),
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
        self.input
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
        self.master
            .as_ref()
            .ok_or("Terminal is stopped")?
            .resize(size(cols, rows))
            .map_err(|e| e.to_string())?;
        self.flow
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .record(Record::Size(cols, rows));
        Ok(())
    }

    pub fn status(&mut self) -> Result<Status, String> {
        if matches!(self.status, Status::Running) {
            if let Some(exit) = self.child.try_wait().map_err(|e| e.to_string())? {
                self.status = Status::Exited {
                    code: exit.exit_code(),
                };
            }
        }
        Ok(self.status.clone())
    }

    pub fn close(&mut self) -> Result<(), String> {
        if self.closed {
            return Ok(());
        }
        let (lock, changed) = &*self.flow;
        lock.lock().unwrap().closed = true;
        changed.notify_all();
        // portable-pty starts a new session. Include foreground and background
        // job-control groups, not only the shell's initial process group.
        // Escaped setsid() descendants remain a documented stage-one blocker.
        terminate_session(self.pid)?;
        let _ = self.child.kill();
        self.input.take();
        self.master.take();
        self.child.wait().map_err(|e| e.to_string())?;
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

fn terminate_session(session: i32) -> Result<(), String> {
    for entry in std::fs::read_dir("/proc")
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // /proc stat's comm may contain spaces and parentheses; fields after the
        // last ')' are state, ppid, pgrp, session. This is not ANSI parsing.
        let Some((_, fields)) = stat.rsplit_once(')') else {
            continue;
        };
        if fields
            .split_whitespace()
            .nth(3)
            .and_then(|s| s.parse::<i32>().ok())
            == Some(session)
        {
            let result = unsafe { libc::kill(pid, libc::SIGKILL) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error.to_string());
                }
            }
        }
    }
    Ok(())
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
