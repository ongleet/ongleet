use super::{
    super::{
        ProcessChild, ProcessExit, ProcessLaunch, ProcessMode, ProcessResources, ProcessService,
    },
    native::*,
};
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal},
    mem::{size_of, zeroed},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{Arc, Mutex},
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{
        Console::*, JobObjects::*, SystemServices::JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO,
        Threading::*, IO::*,
    },
};

/// MS CRT argument quoting, distinct from shell quoting.
pub fn quote(argument: &str) -> String {
    let mut output = String::from("\"");
    let mut slashes = 0;
    for ch in argument.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        if ch == '"' {
            output.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            output.extend(std::iter::repeat_n('\\', slashes));
        }
        slashes = 0;
        output.push(ch);
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}
pub fn resolve(
    path: &Path,
    cwd: Option<&Path>,
    env: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let extensions = ["", ".exe", ".com", ".cmd", ".bat"];
    let bases: Vec<PathBuf> = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else if path.components().count() > 1 {
        vec![cwd.unwrap_or(Path::new(".")).join(path)]
    } else {
        let search = env
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var("PATH").ok())
            .unwrap_or_default();
        std::env::split_paths(&search)
            .map(|base| base.join(path))
            .collect()
    };
    for base in bases {
        for extension in extensions {
            let candidate = if extension.is_empty() || base.extension().is_some() {
                base.clone()
            } else {
                PathBuf::from(format!("{}{extension}", base.display()))
            };
            if candidate.is_file() {
                return std::fs::canonicalize(candidate).map_err(|e| e.to_string());
            }
        }
    }
    Err(format!("Executable unavailable: {}", path.display()))
}
pub struct Job {
    handle: Handle,
    port: Handle,
    stopped: Mutex<bool>,
}
impl Job {
    pub fn new() -> io::Result<Arc<Self>> {
        unsafe {
            let handle = Handle::new(CreateJobObjectW(null(), null()))?;
            let port = Handle::new(CreateIoCompletionPort(
                INVALID_HANDLE_VALUE,
                null_mut(),
                0,
                1,
            ))?;
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            check(SetInformationJobObject(
                handle.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ))?;
            let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
                CompletionKey: 1usize as *mut _,
                CompletionPort: port.0,
            };
            check(SetInformationJobObject(
                handle.0,
                JobObjectAssociateCompletionPortInformation,
                (&association as *const JOBOBJECT_ASSOCIATE_COMPLETION_PORT).cast(),
                size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
            ))?;
            Ok(Arc::new(Self {
                handle,
                port,
                stopped: Mutex::new(false),
            }))
        }
    }
    pub fn kill(&self) -> io::Result<()> {
        let mut stopped = self.stopped.lock().unwrap();
        if *stopped {
            return Ok(());
        }
        unsafe {
            check(TerminateJobObject(self.handle.0, 1))?;
            loop {
                let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = zeroed();
                check(QueryInformationJobObject(
                    self.handle.0,
                    JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    null_mut(),
                ))?;
                if info.ActiveProcesses == 0 {
                    break;
                }
                let mut message = 0;
                let mut key = 0;
                let mut overlapped = null_mut();
                check(GetQueuedCompletionStatus(
                    self.port.0,
                    &mut message,
                    &mut key,
                    &mut overlapped,
                    INFINITE,
                ))?;
                if message == JOB_OBJECT_MSG_ACTIVE_PROCESS_ZERO {
                    break;
                }
            }
        }
        *stopped = true;
        Ok(())
    }
}
pub struct Child {
    pub process: Handle,
    pub id: u32,
    pub job: Option<Arc<Job>>,
}
impl Child {
    pub fn code(&self, timeout: u32) -> io::Result<Option<u32>> {
        unsafe {
            match WaitForSingleObject(self.process.0, timeout) {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    check(GetExitCodeProcess(self.process.0, &mut code))?;
                    Ok(Some(code))
                }
                _ => Err(io::Error::last_os_error()),
            }
        }
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            let _ = job.kill();
        }
    }
}
fn exit(code: u32) -> ProcessExit {
    ProcessExit {
        code: Some(code as i32),
        exit_code: code as i32,
        description: format!("exit code {code}"),
    }
}
impl ProcessChild for Child {
    fn id(&self) -> u32 {
        self.id
    }
    fn try_wait(&mut self) -> Result<Option<ProcessExit>, String> {
        self.code(0).map(|c| c.map(exit)).map_err(|e| e.to_string())
    }
    fn wait(&mut self) -> Result<ProcessExit, String> {
        self.code(INFINITE)
            .map(|c| exit(c.unwrap()))
            .map_err(|e| e.to_string())
    }
}
struct Attributes {
    storage: Vec<usize>,
}
impl Attributes {
    fn new(count: u32) -> io::Result<Self> {
        let mut length = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut length);
        }
        let mut value = Self {
            storage: vec![0; length.div_ceil(size_of::<usize>())],
        };
        check(unsafe {
            InitializeProcThreadAttributeList(value.pointer(), count, 0, &mut length)
        })?;
        Ok(value)
    }
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
    fn set(
        &mut self,
        attribute: u32,
        value: *const std::ffi::c_void,
        length: usize,
    ) -> io::Result<()> {
        check(unsafe {
            UpdateProcThreadAttribute(
                self.pointer(),
                0,
                attribute as usize,
                value,
                length,
                null_mut(),
                null(),
            )
        })
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.pointer());
        }
    }
}

pub fn spawn(
    mut launch: ProcessLaunch,
    console: Option<HPCON>,
    stdio: Option<[HANDLE; 3]>,
    contained: bool,
    detached: bool,
) -> Result<Child, String> {
    let executable = resolve(&launch.executable, launch.cwd.as_deref(), &launch.env)?;
    let extension = executable
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let executable = if matches!(extension.as_str(), "cmd" | "bat") {
        // Use PowerShell's literal expression quoting to invoke Windows shims.
        // Argument strings are never interpolated as shell syntax.
        let script = format!(
            "& {} {}; exit $LASTEXITCODE",
            super::paths::ps_quote(&executable.to_string_lossy()),
            launch
                .args
                .iter()
                .map(|s| super::paths::ps_quote(s))
                .collect::<Vec<_>>()
                .join(" ")
        );
        launch.args = vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-Command".into(),
            script,
        ];
        PathBuf::from(super::paths::powershell())
    } else {
        executable
    };
    let mut command = wide(
        std::iter::once(executable.to_string_lossy().into_owned())
            .chain(launch.args)
            .map(|s| quote(&s))
            .collect::<Vec<_>>()
            .join(" "),
    )
    .map_err(|e| e.to_string())?;
    let application = wide(&executable).map_err(|e| e.to_string())?;
    let cwd = launch
        .cwd
        .as_ref()
        .map(wide)
        .transpose()
        .map_err(|e| e.to_string())?;
    // Environment keys are case insensitive on Windows, including Path/PATH.
    let mut environment = BTreeMap::<String, (String, std::ffi::OsString)>::new();
    for (key, value) in std::env::vars_os() {
        let text = key.to_string_lossy().into_owned();
        environment.insert(text.to_uppercase(), (text, value));
    }
    for (key, value) in launch.env {
        if key.contains(['=', '\0']) {
            return Err("Invalid environment variable name".into());
        }
        environment.insert(key.to_uppercase(), (key, value.into()));
    }
    let mut block = Vec::new();
    for (_, (key, value)) in environment {
        let mut pair = std::ffi::OsString::from(key);
        pair.push("=");
        pair.push(value);
        block.extend(wide(pair).map_err(|e| e.to_string())?);
    }
    block.push(0);
    let job = if contained {
        Some(Job::new().map_err(|e| e.to_string())?)
    } else {
        None
    };
    unsafe {
        let count = u32::from(console.is_some()) + u32::from(stdio.is_some());
        let mut attributes = if count > 0 {
            Some(Attributes::new(count).map_err(|e| e.to_string())?)
        } else {
            None
        };
        if let Some(console) = console {
            attributes
                .as_mut()
                .unwrap()
                .set(
                    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                    console as *const _,
                    size_of::<HPCON>(),
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(handles) = stdio.as_ref() {
            attributes
                .as_mut()
                .unwrap()
                .set(
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
                    handles.as_ptr().cast(),
                    size_of::<[HANDLE; 3]>(),
                )
                .map_err(|e| e.to_string())?;
        }
        let mut startup: STARTUPINFOEXW = zeroed();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        if let Some(attrs) = &mut attributes {
            startup.lpAttributeList = attrs.pointer();
        }
        if let Some([input, output, error]) = stdio {
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = input;
            startup.StartupInfo.hStdOutput = output;
            startup.StartupInfo.hStdError = error;
        }
        let flags = CREATE_SUSPENDED
            | CREATE_UNICODE_ENVIRONMENT
            | EXTENDED_STARTUPINFO_PRESENT
            | if detached {
                DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB
            } else {
                0
            };
        let mut information: PROCESS_INFORMATION = zeroed();
        check(CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            i32::from(stdio.is_some()),
            flags,
            block.as_ptr().cast(),
            cwd.as_ref().map_or(null(), |w| w.as_ptr()),
            &startup.StartupInfo,
            &mut information,
        ))
        .map_err(|e| format!("Cannot create contained process: {e}"))?;
        let process = Handle::new(information.hProcess).map_err(|e| e.to_string())?;
        let thread = Handle::new(information.hThread).map_err(|e| e.to_string())?;
        if let Some(job) = &job {
            if AssignProcessToJobObject(job.handle.0, process.0) == 0 {
                let error = io::Error::last_os_error();
                TerminateProcess(process.0, 1);
                WaitForSingleObject(process.0, INFINITE);
                return Err(format!(
                    "Job assignment failed before user execution: {error}"
                ));
            }
        }
        if ResumeThread(thread.0) == u32::MAX {
            let error = io::Error::last_os_error();
            TerminateProcess(process.0, 1);
            WaitForSingleObject(process.0, INFINITE);
            return Err(error.to_string());
        }
        Ok(Child {
            process,
            id: information.dwProcessId,
            job,
        })
    }
}
fn duplicate(handle: HANDLE) -> io::Result<Handle> {
    let mut result = null_mut();
    unsafe {
        check(DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut result,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        ))?;
    }
    Handle::new(result)
}
struct Captured(Handle);
impl io::Read for Captured {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let mut available = 0;
        unsafe {
            if windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.0 .0,
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            ) == 0
            {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                    return Ok(0);
                }
                return Err(error);
            }
            if available == 0 {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let mut count = 0;
            check(ReadFile(
                self.0 .0,
                bytes.as_mut_ptr(),
                (bytes.len() as u32).min(available),
                &mut count,
                null_mut(),
            ))?;
            Ok(count as usize)
        }
    }
}
fn capture() -> io::Result<(Captured, Handle)> {
    let mut read = null_mut();
    let mut write = null_mut();
    check(unsafe {
        windows_sys::Win32::System::Pipes::CreatePipe(&mut read, &mut write, null(), 65536)
    })?;
    let read = Handle::new(read)?;
    let write = Handle::new(write)?;
    let inherited = duplicate(write.0)?;
    Ok((Captured(read), inherited))
}
impl ProcessService for super::Windows {
    fn has_terminal(&self) -> bool {
        io::stdin().is_terminal() || io::stdout().is_terminal() || io::stderr().is_terminal()
    }
    fn prepare_supervisor(&self) {
        unsafe {
            SetConsoleCtrlHandler(Some(ignore_interrupt), 1);
        }
    }
    fn spawn(&self, launch: ProcessLaunch, mode: ProcessMode) -> Result<ProcessResources, String> {
        let captured = matches!(mode, ProcessMode::Captured);
        let detached = matches!(
            mode,
            ProcessMode::Detached | ProcessMode::TerminalApplication
        );
        let null_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("NUL")
            .map_err(|e| e.to_string())?;
        use std::os::windows::io::AsRawHandle;
        let null_handle = duplicate(null_file.as_raw_handle()).map_err(|e| e.to_string())?;
        let (mut output, mut error) = (None, None);
        let mut owned = Vec::new();
        let handles = if captured {
            let (out, writer) = capture().map_err(|e| e.to_string())?;
            let (err, err_writer) = capture().map_err(|e| e.to_string())?;
            let handles = [null_handle.0, writer.0, err_writer.0];
            owned.extend([writer, err_writer]);
            output = Some(Box::new(out) as Box<dyn io::Read + Send>);
            error = Some(Box::new(err) as Box<dyn io::Read + Send>);
            Some(handles)
        } else if detached {
            Some([null_handle.0; 3])
        } else {
            for id in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                owned.push(
                    duplicate(unsafe { GetStdHandle(id) })
                        .or_else(|_| duplicate(null_handle.0))
                        .map_err(|e| e.to_string())?,
                );
            }
            Some([owned[0].0, owned[1].0, owned[2].0])
        };
        let child = spawn(launch, None, handles, captured, detached)?;
        Ok(ProcessResources {
            child: Box::new(child),
            stdout: output,
            stderr: error,
        })
    }
}
unsafe extern "system" fn ignore_interrupt(kind: u32) -> i32 {
    i32::from(kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT)
}
