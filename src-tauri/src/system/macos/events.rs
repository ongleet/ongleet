//! Owned kqueue waits and a persistent, independent cancellation pipe.
use std::{
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    sync::atomic::{AtomicBool, Ordering},
};
pub fn owned(fd: i32) -> Result<File, String> {
    if fd < 0 {
        Err(io::Error::last_os_error().to_string())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
pub fn event(fd: usize, filter: i16, flags: u16, fflags: u32) -> libc::kevent {
    libc::kevent {
        ident: fd,
        filter,
        flags,
        fflags,
        data: 0,
        udata: std::ptr::null_mut(),
    }
}
pub fn queue(events: &[libc::kevent]) -> Result<File, String> {
    let q = owned(unsafe { libc::kqueue() })?;
    if unsafe { libc::fcntl(q.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    if unsafe {
        libc::kevent(
            q.as_raw_fd(),
            events.as_ptr(),
            events.len() as i32,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    } < 0
    {
        return Err(io::Error::last_os_error().to_string());
    }
    Ok(q)
}
pub fn wait(q: &File) -> Result<libc::kevent, String> {
    loop {
        let mut result = event(0, 0, 0, 0);
        if unsafe {
            libc::kevent(
                q.as_raw_fd(),
                std::ptr::null(),
                0,
                &mut result,
                1,
                std::ptr::null(),
            )
        } >= 0
        {
            return Ok(result);
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e.to_string());
        }
    }
}
pub struct Cancel {
    cancelled: AtomicBool,
    pub read: File,
    write: File,
}
impl Cancel {
    pub fn new() -> Result<Self, String> {
        let mut fds = [0; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
            return Err(io::Error::last_os_error().to_string());
        }
        let read = owned(fds[0])?;
        let write = owned(fds[1])?;
        for f in [&read, &write] {
            if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0
                || unsafe { libc::fcntl(f.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error().to_string());
            }
        }
        Ok(Self {
            cancelled: AtomicBool::new(false),
            read,
            write,
        })
    }
    pub fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel) {
            let value = 1u8;
            loop {
                if unsafe { libc::write(self.write.as_raw_fd(), (&value as *const u8).cast(), 1) }
                    >= 0
                    || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted
                {
                    break;
                }
            }
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn wait_io(&self, file: &File, filter: i16) -> Result<bool, String> {
        if self.cancelled() {
            return Ok(false);
        }
        let q = queue(&[
            event(file.as_raw_fd() as usize, filter, libc::EV_ADD, 0),
            event(
                self.read.as_raw_fd() as usize,
                libc::EVFILT_READ,
                libc::EV_ADD,
                0,
            ),
        ])?;
        let e = wait(&q)?;
        Ok(!self.cancelled() && e.ident != self.read.as_raw_fd() as usize)
    }
}
impl super::super::Cancellation for Cancel {
    fn cancel(&self) {
        self.cancel();
    }
    fn is_cancelled(&self) -> bool {
        self.cancelled()
    }
}
