use super::{
    super::{ControlConnection, ControlEndpoint, ControlListener, ControlTransport},
    native::*,
};
use std::{
    io::{self, Read, Write},
    mem::zeroed,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Pipes::*, IO::*},
};
const FRAME_LIMIT: usize = 1024 * 1024;
fn endpoint(endpoint: &ControlEndpoint) -> Result<&str, String> {
    endpoint
        .0
        .downcast_ref::<String>()
        .map(String::as_str)
        .ok_or_else(|| "Invalid Windows control endpoint".into())
}
fn server(name: &str, first: bool) -> io::Result<Arc<Handle>> {
    let security = Security::private(false)?;
    Ok(Arc::new(Handle::new(unsafe {
        CreateNamedPipeW(
            wide(name)?.as_ptr(),
            PIPE_ACCESS_DUPLEX
                | FILE_FLAG_OVERLAPPED
                | if first {
                    FILE_FLAG_FIRST_PIPE_INSTANCE
                } else {
                    0
                },
            PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            65536,
            65536,
            2000,
            &security.attributes(),
        )
    })?))
}
struct Connection {
    io: PipeIo,
    server: bool,
    read_timeout: Mutex<u32>,
    write_timeout: Mutex<u32>,
    pending: Vec<u8>,
    offset: usize,
    eof: bool,
    finished: AtomicBool,
}
impl Connection {
    fn new(handle: Arc<Handle>, server: bool) -> io::Result<Self> {
        Ok(Self {
            io: PipeIo {
                handle,
                cancel: Cancel::new()?,
            },
            server,
            read_timeout: Mutex::new(2000),
            write_timeout: Mutex::new(2000),
            pending: Vec::new(),
            offset: 0,
            eof: false,
            finished: AtomicBool::new(false),
        })
    }
    fn transfer_all(&self, bytes: &mut [u8], write: bool, timeout: u32) -> io::Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout as u64);
        let mut offset = 0;
        while offset < bytes.len() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let n = self.io.transfer(
                unsafe { bytes.as_mut_ptr().add(offset) },
                bytes.len() - offset,
                write,
                millis(remaining),
            )?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            offset += n;
        }
        Ok(())
    }
}
fn millis(duration: Duration) -> u32 {
    duration.as_millis().max(1).min((u32::MAX - 1) as u128) as u32
}
impl Read for Connection {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() || self.eof {
            return Ok(0);
        }
        if self.offset == self.pending.len() {
            let timeout = *self.read_timeout.lock().unwrap();
            let deadline = std::time::Instant::now() + Duration::from_millis(timeout as u64);
            let mut header = [0u8; 4];
            self.transfer_all(&mut header, false, timeout)?;
            let count = u32::from_le_bytes(header) as usize;
            if count == 0 {
                self.eof = true;
                // Transport-level EOF acknowledgement lets the sender close its
                // server handle only after all preceding bytes were consumed.
                self.transfer_all(
                    &mut u32::MAX.to_le_bytes(),
                    true,
                    *self.write_timeout.lock().unwrap(),
                )?;
                return Ok(0);
            }
            if count > FRAME_LIMIT {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Control frame exceeds 1 MiB",
                ));
            }
            let mut frame = vec![0u8; count];
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(io::ErrorKind::TimedOut.into());
            }
            self.transfer_all(&mut frame, false, millis(remaining))?;
            self.pending = frame;
            self.offset = 0;
        }
        let count = bytes.len().min(self.pending.len() - self.offset);
        bytes[..count].copy_from_slice(&self.pending[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}
impl Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.finished.load(Ordering::Acquire) {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        let count = bytes.len().min(FRAME_LIMIT);
        let mut frame = Vec::with_capacity(count + 4);
        frame.extend_from_slice(&(count as u32).to_le_bytes());
        frame.extend_from_slice(&bytes[..count]);
        self.transfer_all(&mut frame, true, *self.write_timeout.lock().unwrap())?;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl ControlConnection for Connection {
    fn verify_owner(&self) -> Result<(), String> {
        let mut pid = 0;
        check(unsafe {
            if self.server {
                GetNamedPipeClientProcessId(self.io.handle.0, &mut pid)
            } else {
                GetNamedPipeServerProcessId(self.io.handle.0, &mut pid)
            }
        })
        .and_then(|_| verify_pid(pid))
        .map_err(|e| e.to_string())
    }
    fn read_timeout(&self, timeout: Duration) -> io::Result<()> {
        *self.read_timeout.lock().unwrap() = millis(timeout);
        Ok(())
    }
    fn write_timeout(&self, timeout: Duration) -> io::Result<()> {
        *self.write_timeout.lock().unwrap() = millis(timeout);
        Ok(())
    }
    fn finish_write(&self) -> io::Result<()> {
        if self.finished.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let timeout = *self.write_timeout.lock().unwrap();
        self.transfer_all(&mut [0u8; 4], true, timeout)?;
        let mut acknowledgement = [0u8; 4];
        self.transfer_all(&mut acknowledgement, false, timeout)?;
        if u32::from_le_bytes(acknowledgement) != u32::MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid pipe half-close acknowledgement",
            ));
        }
        Ok(())
    }
}
struct Listener {
    name: String,
    next: Mutex<Option<Arc<Handle>>>,
}
impl ControlListener for Listener {
    fn accept(&self) -> io::Result<Box<dyn ControlConnection>> {
        let mut next = self.next.lock().unwrap();
        let handle = match next.take() {
            Some(handle) => handle,
            None => server(&self.name, false)?,
        };
        let cancel = Cancel::new()?;
        let event = Handle::event()?;
        let mut operation: OVERLAPPED = unsafe { zeroed() };
        operation.hEvent = event.0;
        let started = unsafe { ConnectNamedPipe(handle.0, &mut operation) };
        if started == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
            if unsafe { GetLastError() } != ERROR_IO_PENDING {
                return Err(io::Error::last_os_error());
            }
            unsafe {
                complete(
                    handle.0,
                    &mut operation,
                    started,
                    &cancel,
                    windows_sys::Win32::System::Threading::INFINITE,
                )
            }?;
        }
        *next = Some(server(&self.name, false)?);
        let connection = Connection::new(handle, true)?;
        connection.verify_owner().map_err(io::Error::other)?;
        Ok(Box::new(connection))
    }
}
impl ControlTransport for super::Windows {
    fn connect(
        &self,
        endpoint_value: &ControlEndpoint,
    ) -> Result<Box<dyn ControlConnection>, String> {
        let name = wide(endpoint(endpoint_value)?).map_err(|e| e.to_string())?;
        let handle = Handle::new(unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                null_mut(),
            )
        })
        .map_err(|e| e.to_string())?;
        let connection = Connection::new(Arc::new(handle), false).map_err(|e| e.to_string())?;
        connection.verify_owner()?;
        Ok(Box::new(connection))
    }
    fn listen(&self, endpoint_value: &ControlEndpoint) -> Result<Box<dyn ControlListener>, String> {
        let name = endpoint(endpoint_value)?.to_owned();
        let handle = server(&name, true).map_err(|e| e.to_string())?;
        Ok(Box::new(Listener {
            name,
            next: Mutex::new(Some(handle)),
        }))
    }
}
