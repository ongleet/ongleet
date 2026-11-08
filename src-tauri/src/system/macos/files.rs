use super::super::{FileLock, FileWatch, PrivateOpen, SystemFiles};
use std::{
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::Path,
};
struct Lock(File);
impl FileLock for Lock {}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
impl SystemFiles for super::Macos {
    fn atomic_replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::rename(source, destination)
    }
    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        File::open(directory)?.sync_all()
    }
    fn private_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        fs::DirBuilder::new()
            .recursive(recursive)
            .mode(0o700)
            .create(path)
    }
    fn private_permissions(&self, path: &Path, directory: bool) -> io::Result<()> {
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
    }
    fn open_private(&self, path: &Path, mode: PrivateOpen) -> io::Result<File> {
        let mut options = OpenOptions::new();
        options.write(true).mode(0o600);
        match mode {
            PrivateOpen::New => {
                options.create_new(true);
            }
            PrivateOpen::Replace => {
                options.create(true).truncate(true);
            }
            PrivateOpen::Lock => {
                options.read(true).create(true).truncate(false);
            }
            PrivateOpen::Diagnostic => {
                options
                    .create(true)
                    .truncate(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
            }
        }
        options.open(path)
    }
    fn try_lock(&self, path: &Path) -> Result<Box<dyn FileLock>, String> {
        let file = self
            .open_private(path, PrivateOpen::Lock)
            .map_err(|e| e.to_string())?;
        file.try_lock().map_err(|e| e.to_string())?;
        Ok(Box::new(Lock(file)))
    }
    fn nonce(&self) -> Result<String, String> {
        let mut bytes = [0u8; 16];
        File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|e| e.to_string())?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }
    fn open_editor(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
            .open(path)
    }
    fn revision(&self, bytes: &[u8], metadata: &fs::Metadata, _path: &Path) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        (
            metadata.dev(),
            metadata.ino(),
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.mode(),
            metadata.uid(),
            metadata.gid(),
        )
            .hash(&mut hash);
        format!("{:016x}", hash.finish())
    }
    fn editable(&self, metadata: &fs::Metadata) -> bool {
        metadata.uid() == unsafe { libc::geteuid() }
            && metadata.nlink() == 1
            && metadata.mode() & 0o222 != 0
    }
    fn preserve_attributes(&self, file: &File, metadata: &fs::Metadata) -> Result<(), String> {
        if file.metadata().map_err(|e| e.to_string())?.gid() != metadata.gid()
            && unsafe { libc::fchown(file.as_raw_fd(), !0, metadata.gid()) } != 0
        {
            return Err(format!(
                "Cannot preserve file group: {}",
                io::Error::last_os_error()
            ));
        }
        file.set_permissions(fs::Permissions::from_mode(metadata.mode() & 0o777))
            .map_err(|e| e.to_string())
    }
    fn watch(
        &self,
        path: String,
        changed: Box<dyn Fn() + Send>,
    ) -> Result<Box<dyn FileWatch>, String> {
        super::watch::Watch::start(path, String::new(), changed)
            .map(|watch| Box::new(watch) as Box<dyn FileWatch>)
    }
}
