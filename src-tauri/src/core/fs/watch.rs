//! Linux directory notifications, coalesced until the frontend rereads the file.
use std::{
    ffi::CString,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub struct Watch {
    pub path: String,
    pub token: String,
    pending: Arc<AtomicBool>,
    stop: Arc<OwnedFd>,
    worker: Option<JoinHandle<()>>,
}
fn fd(value: i32) -> Result<OwnedFd, String> {
    if value < 0 {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(value) })
    }
}
impl Watch {
    pub fn start(
        path: String,
        token: String,
        changed: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let target = Path::new(&path);
        if !target.is_absolute() {
            return Err("Choose an absolute file path".into());
        }
        let parent = target.parent().ok_or("File has no parent directory")?;
        let name = target
            .file_name()
            .ok_or("File has no name")?
            .as_bytes()
            .to_vec();
        let directory = CString::new(parent.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
        let input = fd(unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) })?;
        let mask = libc::IN_CLOSE_WRITE
            | libc::IN_MOVED_TO
            | libc::IN_MOVED_FROM
            | libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_ATTRIB
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF;
        if unsafe {
            libc::inotify_add_watch(
                input.as_raw_fd(),
                directory.as_ptr(),
                mask | libc::IN_ONLYDIR,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let stop = Arc::new(fd(unsafe {
            libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK)
        })?);
        let pending = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_pending = pending.clone();
        let worker = std::thread::Builder::new()
            .name("file-watch".into())
            .spawn(move || {
                let mut polls = [
                    libc::pollfd {
                        fd: input.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                    libc::pollfd {
                        fd: worker_stop.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    },
                ];
                let mut buffer = [0u8; 16384];
                loop {
                    let result = unsafe { libc::poll(polls.as_mut_ptr(), 2, -1) };
                    if result < 0 {
                        if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                        {
                            continue;
                        }
                        break;
                    }
                    if polls[1].revents != 0 {
                        break;
                    }
                    if polls[0].revents & libc::POLLIN == 0 {
                        break;
                    }
                    let count = unsafe {
                        libc::read(input.as_raw_fd(), buffer.as_mut_ptr().cast(), buffer.len())
                    };
                    if count <= 0 {
                        continue;
                    }
                    let mut offset = 0;
                    let mut relevant = false;
                    let header = std::mem::size_of::<libc::inotify_event>();
                    while offset + header <= count as usize {
                        let event = unsafe {
                            std::ptr::read_unaligned(
                                buffer.as_ptr().add(offset).cast::<libc::inotify_event>(),
                            )
                        };
                        let end = offset + header + event.len as usize;
                        if end > count as usize {
                            break;
                        }
                        let event_name = &buffer[offset + header..end];
                        let length = event_name
                            .iter()
                            .position(|b| *b == 0)
                            .unwrap_or(event_name.len());
                        relevant |= event.mask
                            & (libc::IN_Q_OVERFLOW
                                | libc::IN_IGNORED
                                | libc::IN_DELETE_SELF
                                | libc::IN_MOVE_SELF)
                            != 0
                            || event_name[..length] == name;
                        offset = end;
                    }
                    if relevant && !worker_pending.swap(true, Ordering::AcqRel) {
                        changed();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            path,
            token,
            pending,
            stop,
            worker: Some(worker),
        })
    }
    pub fn acknowledge(&self) {
        self.pending.store(false, Ordering::Release);
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        let value = 1u64;
        unsafe {
            libc::write(self.stop.as_raw_fd(), (&value as *const u64).cast(), 8);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn watches_replacements_and_recreation_without_reporting_siblings() {
        let root = std::env::temp_dir().join(format!("devcenter-watch-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("file.txt");
        std::fs::write(&path, "original").unwrap();
        let (send, receive) = mpsc::channel();
        let watch = Watch::start(path.to_str().unwrap().into(), "test".into(), move || {
            send.send(()).unwrap();
        })
        .unwrap();
        std::fs::write(root.join("sibling"), "unrelated").unwrap();
        assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
        std::fs::write(&path, "in-place").unwrap();
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        // Changes stay coalesced until a reader acknowledges the notification.
        std::fs::write(&path, "coalesced").unwrap();
        assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
        watch.acknowledge();
        let temporary = root.join("replacement");
        std::fs::write(&temporary, "atomic replacement").unwrap();
        std::fs::rename(&temporary, &path).unwrap();
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "atomic replacement"
        );
        watch.acknowledge();
        std::fs::remove_file(&path).unwrap();
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        watch.acknowledge();
        std::fs::write(&path, "recreated").unwrap();
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(watch);
        std::fs::write(&path, "after stop").unwrap();
        assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
