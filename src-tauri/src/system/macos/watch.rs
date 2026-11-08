use super::events::{self, Cancel};
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};
pub struct Watch {
    pending: Arc<AtomicBool>,
    stop: Arc<Cancel>,
    worker: Option<JoinHandle<()>>,
}
fn stamp(path: &PathBuf) -> Option<(u64, u64, i64, i64, u64)> {
    std::fs::symlink_metadata(path)
        .ok()
        .map(|m| (m.dev(), m.ino(), m.mtime(), m.mtime_nsec(), m.len()))
}
impl Watch {
    pub fn start(
        path: String,
        _token: String,
        changed: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let path = PathBuf::from(path);
        if !path.is_absolute() || path.file_name().is_none() {
            return Err("Choose an absolute file path".into());
        }
        let stop = Arc::new(Cancel::new()?);
        let pending = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_pending = pending.clone();
        let worker = std::thread::Builder::new()
            .name("file-watch".into())
            .spawn(move || {
                let mut previous = stamp(&path);
                while !worker_stop.cancelled() {
                    // Watch the target for in-place writes and ancestors for replacement,
                    // deletion and recreation. Rebuild after every directory event.
                    let handles: Vec<File> = path
                        .ancestors()
                        .filter_map(|p| {
                            OpenOptions::new()
                                .read(true)
                                .custom_flags(libc::O_EVTONLY | libc::O_CLOEXEC)
                                .open(p)
                                .ok()
                        })
                        .collect();
                    let mut changes: Vec<_> = handles
                        .iter()
                        .map(|f| {
                            events::event(
                                f.as_raw_fd() as usize,
                                libc::EVFILT_VNODE,
                                libc::EV_ADD | libc::EV_CLEAR,
                                libc::NOTE_WRITE
                                    | libc::NOTE_RENAME
                                    | libc::NOTE_DELETE
                                    | libc::NOTE_ATTRIB,
                            )
                        })
                        .collect();
                    changes.push(events::event(
                        worker_stop.read.as_raw_fd() as usize,
                        libc::EVFILT_READ,
                        libc::EV_ADD,
                        0,
                    ));
                    let Ok(q) = events::queue(&changes) else {
                        break;
                    };
                    let current = stamp(&path);
                    if current != previous {
                        previous = current;
                        if !worker_pending.swap(true, Ordering::AcqRel) {
                            changed();
                        }
                    }
                    let Ok(event) = events::wait(&q) else { break };
                    if event.ident == worker_stop.read.as_raw_fd() as usize {
                        break;
                    }
                    // A target event can carry changes without a distinct timestamp.
                    let target = handles
                        .first()
                        .is_some_and(|f| f.as_raw_fd() as usize == event.ident)
                        && path.exists();
                    let current = stamp(&path);
                    if target || current != previous {
                        previous = current;
                        if !worker_pending.swap(true, Ordering::AcqRel) {
                            changed();
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            pending,
            stop,
            worker: Some(worker),
        })
    }
}
impl super::super::FileWatch for Watch {
    fn acknowledge(&self) {
        self.pending.store(false, Ordering::Release);
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
