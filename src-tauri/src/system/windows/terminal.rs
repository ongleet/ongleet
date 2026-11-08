use super::{
    super::{
        Cancellation, Launch, ProcessLaunch, TerminalControl, TerminalIo, TerminalReader,
        TerminalService, TerminalWait, TerminalWriter,
    },
    native::*,
};
use std::{
    io,
    mem::zeroed,
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Console::*, Pipes::*, Threading::*, IO::*},
};

/// The ConPTY ends are synchronous. Only the application's ends use overlapped
/// I/O; this is required by the pseudoconsole API.
pub fn pair(write_from_application: bool) -> io::Result<(PipeIo, Handle)> {
    let name = wide(format!(
        r"\\.\pipe\ongleet-pty-{}-{}",
        std::process::id(),
        super::super::SystemFiles::nonce(&super::Windows::new()).map_err(io::Error::other)?
    ))?;
    let security = Security::private(false)?;
    let access = if write_from_application {
        PIPE_ACCESS_OUTBOUND
    } else {
        PIPE_ACCESS_INBOUND
    };
    let app = Arc::new(Handle::new(unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            access | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            0,
            &security.attributes(),
        )
    })?);
    let peer = Handle::new(unsafe {
        CreateFileW(
            name.as_ptr(),
            if write_from_application {
                GENERIC_READ
            } else {
                GENERIC_WRITE
            },
            0,
            &security.attributes(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    })?;
    // Opening the client before ConnectNamedPipe yields ERROR_PIPE_CONNECTED.
    let event = Handle::event()?;
    let mut operation: OVERLAPPED = unsafe { zeroed() };
    operation.hEvent = event.0;
    if unsafe { ConnectNamedPipe(app.0, &mut operation) } == 0
        && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED
    {
        if unsafe { GetLastError() } != ERROR_IO_PENDING {
            return Err(io::Error::last_os_error());
        }
        let cancel = Cancel::new()?;
        unsafe { complete(app.0, &mut operation, 0, &cancel, 2000) }?;
    }
    Ok((
        PipeIo {
            handle: app,
            cancel: Cancel::new()?,
        },
        peer,
    ))
}
struct Reader(PipeIo);
impl TerminalReader for Reader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self
            .0
            .transfer(bytes.as_mut_ptr(), bytes.len(), false, INFINITE)
        {
            Err(_) if self.0.cancel.is_cancelled() => Ok(0),
            value => value,
        }
    }
    fn wait(&self) -> Result<bool, String> {
        Ok(!self.0.cancel.is_cancelled())
    }
}
struct Writer(PipeIo);
impl TerminalWriter for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .transfer(bytes.as_ptr() as *mut _, bytes.len(), true, INFINITE)
    }
    fn wait(&self) -> Result<bool, String> {
        Ok(!self.0.cancel.is_cancelled())
    }
}
struct Console(HPCON);
unsafe impl Send for Console {}
impl Drop for Console {
    fn drop(&mut self) {
        unsafe {
            ClosePseudoConsole(self.0);
        }
    }
}
struct Control {
    console: Mutex<Option<Console>>,
    job: Arc<super::process::Job>,
    released: AtomicBool,
}
impl Control {
    fn close_console(&self) {
        if let Some(console) = self.console.lock().unwrap().take() {
            // ClosePseudoConsole can wait for its output pipe. Keep the reader
            // independent and never hold a registry or control lock while closing.
            std::thread::spawn(move || drop(console));
        }
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        let _ = self.job.kill();
        self.close_console();
    }
}
impl TerminalControl for Control {
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        if let Some(console) = self.console.lock().unwrap().as_ref() {
            let result = unsafe {
                ResizePseudoConsole(
                    console.0,
                    COORD {
                        X: cols as i16,
                        Y: rows as i16,
                    },
                )
            };
            if result < 0 {
                return Err(format!("ConPTY resize failed: {result}"));
            }
        }
        Ok(())
    }
    fn stop(&self) -> Result<(), String> {
        let result = self.job.kill().map_err(|e| e.to_string());
        self.close_console();
        result
    }
    fn release(&self) {
        self.released.store(true, Ordering::Release);
    }
    #[cfg(test)]
    fn group_directory(&self) -> std::path::PathBuf {
        std::path::PathBuf::new()
    }
    #[cfg(test)]
    fn released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}
struct Wait(super::process::Child);
impl TerminalWait for Wait {
    fn wait(self: Box<Self>) -> Result<u32, String> {
        self.0
            .code(INFINITE)
            .map(|code| code.unwrap())
            .map_err(|e| e.to_string())
    }
}
impl TerminalService for super::Windows {
    fn post_exit_drain_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(2))
    }
    fn spawn(&self, mut launch: Launch) -> Result<TerminalIo, String> {
        let (input, con_input) = pair(true).map_err(|e| e.to_string())?;
        let (output, con_output) = pair(false).map_err(|e| e.to_string())?;
        let mut console = 0;
        let result = unsafe {
            CreatePseudoConsole(
                COORD {
                    X: launch.cols as i16,
                    Y: launch.rows as i16,
                },
                con_input.0,
                con_output.0,
                0,
                &mut console,
            )
        };
        if result < 0 {
            return Err(format!("ConPTY creation failed: {result}"));
        }
        let console = Console(console);
        // Close creation-only ends even on launch failure, before console teardown.
        drop(con_input);
        drop(con_output);
        if !launch
            .env
            .keys()
            .any(|key| key.eq_ignore_ascii_case("PATH"))
        {
            let application = super::super::SystemPaths::application_executable(self)?;
            let directory = application
                .parent()
                .ok_or("Application directory unavailable")?;
            let inherited = std::env::var_os("PATH").unwrap_or_default();
            let path = std::env::join_paths(
                std::iter::once(directory.to_path_buf()).chain(std::env::split_paths(&inherited)),
            )
            .map_err(|e| e.to_string())?;
            launch
                .env
                .insert("PATH".into(), path.to_string_lossy().into_owned());
        }
        launch
            .env
            .entry("TERM".into())
            .or_insert_with(|| "xterm-256color".into());
        launch
            .env
            .entry("COLORTERM".into())
            .or_insert_with(|| "truecolor".into());
        let child = super::process::spawn(
            self,
            ProcessLaunch {
                executable: launch
                    .executable
                    .unwrap_or_else(super::paths::powershell)
                    .into(),
                args: launch.args,
                env: launch.env,
                cwd: Some(launch.cwd.into()),
            },
            Some(console.0),
            None,
            true,
            false,
        )?;
        let control = Arc::new(Control {
            console: Mutex::new(Some(console)),
            job: child.job.as_ref().unwrap().clone(),
            released: AtomicBool::new(false),
        });
        Ok(TerminalIo {
            cancel_input: input.cancel.clone(),
            cancel_read: output.cancel.clone(),
            reader: Box::new(Reader(output)),
            writer: Box::new(Writer(input)),
            control,
            wait: Box::new(Wait(child)),
        })
    }
}
