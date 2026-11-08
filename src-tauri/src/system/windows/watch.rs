use super::{
    super::{Cancellation, FileWatch},
    native::*,
};
use std::{
    io,
    mem::zeroed,
    path::PathBuf,
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::JoinHandle,
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Threading::INFINITE, IO::*},
};
pub struct Watch {
    cancel: Arc<Cancel>,
    pending: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Watch {
    pub fn start(path: String, changed: Box<dyn Fn() + Send>) -> Result<Self, String> {
        let path = PathBuf::from(path);
        if !path.is_absolute() || path.file_name().is_none() {
            return Err("Choose an absolute file path".into());
        }
        // Observe from an existing ancestor with subtree notifications. This
        // survives replacement/removal/recreation of the target's parent too.
        let root = path
            .ancestors()
            .skip(1)
            .find(|p| p.is_dir())
            .ok_or("Watch ancestor unavailable")?;
        let handle = Arc::new(
            Handle::new(unsafe {
                CreateFileW(
                    wide(root).map_err(|e| e.to_string())?.as_ptr(),
                    FILE_LIST_DIRECTORY,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
                    null_mut(),
                )
            })
            .map_err(|e| e.to_string())?,
        );
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        let cancel = Cancel::new().map_err(|e| e.to_string())?;
        let pending = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let coalesced = pending.clone();
        let (ready, receive) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            let event = match Handle::event() {
                Ok(event) => event,
                Err(e) => {
                    let _ = ready.send(Err(e.to_string()));
                    return;
                }
            };
            let mut ready = Some(ready);
            let mut bytes = vec![0u32; 16384];
            while !stop.is_cancelled() {
                unsafe {
                    windows_sys::Win32::System::Threading::ResetEvent(event.0);
                }
                let mut operation: OVERLAPPED = unsafe { zeroed() };
                operation.hEvent = event.0;
                let started = unsafe {
                    ReadDirectoryChangesW(
                        handle.0,
                        bytes.as_mut_ptr().cast(),
                        (bytes.len() * 4) as u32,
                        1,
                        FILE_NOTIFY_CHANGE_FILE_NAME
                            | FILE_NOTIFY_CHANGE_DIR_NAME
                            | FILE_NOTIFY_CHANGE_LAST_WRITE
                            | FILE_NOTIFY_CHANGE_SIZE
                            | FILE_NOTIFY_CHANGE_ATTRIBUTES
                            | FILE_NOTIFY_CHANGE_SECURITY,
                        null_mut(),
                        &mut operation,
                        None,
                    )
                };
                if started == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
                    if let Some(ready) = ready.take() {
                        let _ = ready.send(Err(io::Error::last_os_error().to_string()));
                    }
                    break;
                }
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Ok(()));
                }
                let count =
                    match unsafe { complete(handle.0, &mut operation, started, &stop, INFINITE) } {
                        Ok(n) => n as usize,
                        Err(_) => break,
                    };
                let mut matches = count == 0;
                let mut offset = 0;
                while offset + 12 <= count {
                    let raw = unsafe {
                        (bytes.as_ptr().cast::<u8>().add(offset)).cast::<FILE_NOTIFY_INFORMATION>()
                    };
                    let item = unsafe { &*raw };
                    let length = item.FileNameLength as usize / 2;
                    if offset + 12 + length * 2 > count {
                        matches = true;
                        break;
                    }
                    let name = String::from_utf16_lossy(unsafe {
                        std::slice::from_raw_parts(item.FileName.as_ptr(), length)
                    })
                    .to_lowercase();
                    if name == relative || relative.starts_with(&(name + "\\")) {
                        matches = true;
                    }
                    if item.NextEntryOffset == 0 {
                        break;
                    }
                    offset += item.NextEntryOffset as usize;
                }
                if matches && !coalesced.swap(true, Ordering::AcqRel) {
                    changed();
                }
            }
        });
        match receive.recv() {
            Ok(Ok(())) => Ok(Self {
                cancel,
                pending,
                worker: Some(worker),
            }),
            result => {
                cancel.cancel();
                let _ = worker.join();
                Err(format!("Cannot arm file watch: {result:?}"))
            }
        }
    }
}
impl FileWatch for Watch {
    fn acknowledge(&self) {
        self.pending.store(false, Ordering::Release);
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
