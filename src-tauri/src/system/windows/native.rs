use std::{
    ffi::OsStr,
    io,
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{Threading::*, IO::*},
};

pub fn wide(value: impl AsRef<OsStr>) -> io::Result<Vec<u16>> {
    let mut result: Vec<u16> = value.as_ref().encode_wide().collect();
    if result.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in native argument",
        ));
    }
    result.push(0);
    Ok(result)
}
pub fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
pub struct Handle(pub HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Handle {
    pub fn new(raw: HANDLE) -> io::Result<Self> {
        if raw.is_null() || raw == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(raw))
        }
    }
    pub fn event() -> io::Result<Self> {
        Self::new(unsafe { CreateEventW(null(), 1, 0, null()) })
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
pub struct Cancel(pub Handle);
impl Cancel {
    pub fn new() -> io::Result<Arc<Self>> {
        Ok(Arc::new(Self(Handle::event()?)))
    }
}
impl super::super::Cancellation for Cancel {
    fn cancel(&self) {
        unsafe {
            SetEvent(self.0 .0);
        }
    }
    fn is_cancelled(&self) -> bool {
        unsafe { WaitForSingleObject(self.0 .0, 0) == WAIT_OBJECT_0 }
    }
}
#[derive(PartialEq, Eq)]
pub struct Identity {
    pub sid: String,
    pub logon: (u32, i32),
}
pub fn identity(process: HANDLE) -> io::Result<Identity> {
    unsafe {
        let mut token = null_mut();
        check(OpenProcessToken(process, TOKEN_QUERY, &mut token))?;
        let token = Handle::new(token)?;
        let mut length = 0;
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut length);
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        check(GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        ))?;
        let user = &*(buffer.as_ptr().cast::<TOKEN_USER>());
        let mut sid = null_mut();
        check(ConvertSidToStringSidW(user.User.Sid, &mut sid))?;
        let mut n = 0;
        while *sid.add(n) != 0 {
            n += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(sid, n));
        LocalFree(sid.cast());
        let mut stats: TOKEN_STATISTICS = zeroed();
        check(GetTokenInformation(
            token.0,
            TokenStatistics,
            (&mut stats as *mut TOKEN_STATISTICS).cast(),
            size_of::<TOKEN_STATISTICS>() as u32,
            &mut length,
        ))?;
        Ok(Identity {
            sid: text,
            logon: (
                stats.AuthenticationId.LowPart,
                stats.AuthenticationId.HighPart,
            ),
        })
    }
}
pub fn current_identity() -> io::Result<Identity> {
    identity(unsafe { GetCurrentProcess() })
}
pub fn verify_pid(pid: u32) -> io::Result<()> {
    let process = Handle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })?;
    if identity(process.0)? != current_identity()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Pipe peer belongs to a different user or logon",
        ));
    }
    Ok(())
}
pub struct Security {
    descriptor: PSECURITY_DESCRIPTOR,
}
impl Security {
    pub fn private(directory: bool) -> io::Result<Self> {
        let inherit = if directory { "OICI" } else { "" };
        let sddl = wide(format!("D:P(A;{inherit};FA;;;{})", current_identity()?.sid))?;
        let mut descriptor = null_mut();
        check(unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        })?;
        Ok(Self { descriptor })
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor,
            bInheritHandle: 0,
        }
    }
    pub fn apply(&self, path: &std::path::Path) -> io::Result<()> {
        let mut dacl = null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        unsafe {
            check(GetSecurityDescriptorDacl(
                self.descriptor,
                &mut present,
                &mut dacl,
                &mut defaulted,
            ))?;
            let status = SetNamedSecurityInfoW(
                wide(path)?.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                dacl,
                null(),
            );
            if status != 0 {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
        }
        Ok(())
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.descriptor);
        }
    }
}

/// Complete one overlapped operation before its buffer/OVERLAPPED may be freed.
/// Cancellation is event-driven and scoped to this exact operation.
pub unsafe fn complete(
    handle: HANDLE,
    operation: &mut OVERLAPPED,
    _started: i32,
    cancel: &Cancel,
    timeout: u32,
) -> io::Result<u32> {
    let events = [cancel.0 .0, operation.hEvent];
    let result = WaitForMultipleObjects(2, events.as_ptr(), 0, timeout);
    if result != WAIT_OBJECT_0 + 1 {
        CancelIoEx(handle, operation);
        let mut discarded = 0;
        GetOverlappedResult(handle, operation, &mut discarded, 1);
        return Err(if result == WAIT_TIMEOUT {
            io::ErrorKind::TimedOut.into()
        } else {
            io::ErrorKind::Interrupted.into()
        });
    }
    let mut bytes = 0;
    check(GetOverlappedResult(handle, operation, &mut bytes, 0))?;
    Ok(bytes)
}
pub struct PipeIo {
    pub handle: Arc<Handle>,
    pub cancel: Arc<Cancel>,
}
impl PipeIo {
    pub fn transfer(
        &self,
        bytes: *mut u8,
        length: usize,
        write: bool,
        timeout: u32,
    ) -> io::Result<usize> {
        use super::super::Cancellation;
        if self.cancel.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let event = Handle::event()?;
        let mut overlapped: OVERLAPPED = unsafe { zeroed() };
        overlapped.hEvent = event.0;
        let started = unsafe {
            if write {
                WriteFile(
                    self.handle.0,
                    bytes,
                    length.min(u32::MAX as usize) as u32,
                    null_mut(),
                    &mut overlapped,
                )
            } else {
                ReadFile(
                    self.handle.0,
                    bytes,
                    length.min(u32::MAX as usize) as u32,
                    null_mut(),
                    &mut overlapped,
                )
            }
        };
        if started == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_IO_PENDING {
                if !write
                    && matches!(
                        error,
                        ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED
                    )
                {
                    return Ok(0);
                }
                return Err(io::Error::from_raw_os_error(error as i32));
            }
        }
        match unsafe {
            complete(
                self.handle.0,
                &mut overlapped,
                started,
                &self.cancel,
                timeout,
            )
        } {
            Err(e) if matches!(e.raw_os_error(), Some(109 | 232 | 233)) && !write => Ok(0),
            value => value.map(|n| n as usize),
        }
    }
}
