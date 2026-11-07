use super::super::{
    ProcessChild, ProcessExit, ProcessLaunch, ProcessMode, ProcessResources, ProcessService,
};
use std::{
    io::{self, IsTerminal},
    os::{
        fd::AsRawFd,
        unix::process::{CommandExt, ExitStatusExt},
    },
    process::{Child, Command, ExitStatus, Stdio},
};
struct OwnedChild {
    child: Child,
    terminate: bool,
}
fn exit(status: ExitStatus) -> ProcessExit {
    ProcessExit {
        code: status.code(),
        exit_code: status
            .code()
            .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
        description: status.to_string(),
    }
}
impl ProcessChild for OwnedChild {
    fn id(&self) -> u32 {
        self.child.id()
    }
    fn try_wait(&mut self) -> Result<Option<ProcessExit>, String> {
        self.child
            .try_wait()
            .map(|status| status.map(exit))
            .map_err(|e| e.to_string())
    }
    fn wait(&mut self) -> Result<ProcessExit, String> {
        self.child.wait().map(exit).map_err(|e| e.to_string())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.terminate {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            let _ = self.child.wait();
        }
    }
}
fn nonblocking(pipe: &impl AsRawFd) -> Result<(), String> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    Ok(())
}
impl ProcessService for super::Linux {
    fn has_terminal(&self) -> bool {
        io::stdin().is_terminal() || io::stdout().is_terminal() || io::stderr().is_terminal()
    }
    fn prepare_supervisor(&self) {
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
            libc::signal(libc::SIGQUIT, libc::SIG_IGN);
        }
    }
    fn spawn(&self, launch: ProcessLaunch, mode: ProcessMode) -> Result<ProcessResources, String> {
        let mut command = Command::new(launch.executable);
        command.args(launch.args).envs(launch.env);
        if let Some(cwd) = launch.cwd {
            command.current_dir(cwd);
        }
        match mode {
            ProcessMode::Captured => {
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .process_group(0);
            }
            ProcessMode::Inherited => unsafe {
                command.pre_exec(|| {
                    libc::signal(libc::SIGINT, libc::SIG_DFL);
                    libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                    Ok(())
                });
            },
            ProcessMode::Detached | ProcessMode::TerminalApplication => {
                command.stdin(Stdio::null());
                command.stdout(
                    if matches!(mode, ProcessMode::TerminalApplication)
                        && !io::stdout().is_terminal()
                    {
                        Stdio::inherit()
                    } else {
                        Stdio::null()
                    },
                );
                command.stderr(
                    if matches!(mode, ProcessMode::TerminalApplication)
                        && !io::stderr().is_terminal()
                    {
                        Stdio::inherit()
                    } else {
                        Stdio::null()
                    },
                );
                unsafe {
                    command.pre_exec(|| {
                        if libc::setsid() < 0 {
                            return Err(io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
            }
        }
        let child = command.spawn().map_err(|e| e.to_string())?;
        let mut child = OwnedChild {
            child,
            terminate: matches!(mode, ProcessMode::Captured),
        };
        let stdout = if let Some(pipe) = child.child.stdout.take() {
            nonblocking(&pipe)?;
            Some(Box::new(pipe) as Box<dyn io::Read + Send>)
        } else {
            None
        };
        let stderr = if let Some(pipe) = child.child.stderr.take() {
            nonblocking(&pipe)?;
            Some(Box::new(pipe) as Box<dyn io::Read + Send>)
        } else {
            None
        };
        Ok(ProcessResources {
            child: Box::new(child),
            stdout,
            stderr,
        })
    }
}
