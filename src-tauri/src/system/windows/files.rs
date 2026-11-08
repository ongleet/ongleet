use super::{
    super::{FileLock, FileWatch, PrivateOpen, SystemFiles},
    native::*,
};
use std::{
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io,
    mem::zeroed,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::{Foundation::*, Security::Cryptography::*, Storage::FileSystem::*};
#[derive(Hash, PartialEq, Eq)]
pub struct Identity(u32, u32, u32, u64, u64);
pub fn identity(path: &Path) -> io::Result<Identity> {
    let file = File::open(path)?;
    let mut info = unsafe { zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    check(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) })?;
    Ok(Identity(
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
        ((info.ftLastWriteTime.dwHighDateTime as u64) << 32)
            | info.ftLastWriteTime.dwLowDateTime as u64,
        ((info.nFileSizeHigh as u64) << 32) | info.nFileSizeLow as u64,
    ))
}
struct Lock(File);
impl FileLock for Lock {}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}
impl SystemFiles for super::Windows {
    fn move_directory(&self, source: &Path, destination: &Path) -> io::Result<()> {
        let parent = destination.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Migration destination has no parent",
            )
        })?;
        let name = destination.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Migration destination has no name",
            )
        })?;
        let source = wide(source)?;
        let destination = wide(name)?;
        let handle = Handle::new(unsafe {
            CreateFileW(
                source.as_ptr(),
                DELETE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        })
        .map_err(|e| io::Error::new(e.kind(), format!("Open migration source: {e}")))?;
        let parent = Handle::new(unsafe {
            CreateFileW(
                wide(parent)?.as_ptr(),
                FILE_TRAVERSE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                null_mut(),
            )
        })
        .map_err(|e| io::Error::new(e.kind(), format!("Open migration destination parent: {e}")))?;
        let offset = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let length = offset + (destination.len() - 1) * 2;
        let mut storage = vec![0usize; length.div_ceil(std::mem::size_of::<usize>())];
        let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        unsafe {
            // FILE_RENAME_FLAG_POSIX_SEMANTICS: keep existing writer handles
            // valid while the directory name changes. No replacement flag is set.
            (*info).Anonymous.Flags = 2;
            (*info).RootDirectory = parent.0;
            (*info).FileNameLength = ((destination.len() - 1) * 2) as u32;
            std::ptr::copy_nonoverlapping(
                destination.as_ptr(),
                (*info).FileName.as_mut_ptr(),
                destination.len() - 1,
            );
            check(SetFileInformationByHandle(
                handle.0,
                FileRenameInfoEx,
                info.cast(),
                length as u32,
            ))
            .map_err(|e| {
                io::Error::new(e.kind(), format!("Rename locked migration directory: {e}"))
            })
        }
    }
    fn atomic_replace(&self, source: &Path, destination: &Path) -> io::Result<()> {
        let source = wide(source)?;
        let destination = wide(destination)?;
        // Same-volume rename supports an open temporary handle shared for
        // deletion. ReplaceFileW opens the replacement exclusively and fails
        // while shared callers still hold the flushed file for its metadata.
        check(unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
    }

    fn sync_directory(&self, _: &Path) -> io::Result<()> {
        // Windows does not expose POSIX directory fsync; file flush precedes the
        // atomic replacement, and new-name moves request write-through.
        Ok(())
    }
    fn private_directory(&self, path: &Path, recursive: bool) -> io::Result<()> {
        let security = Security::private(true)?;
        if recursive {
            if let Some(parent) = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty() && !p.exists())
            {
                self.private_directory(parent, true)?;
            }
        }
        if unsafe { CreateDirectoryW(wide(path)?.as_ptr(), &security.attributes()) } == 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::AlreadyExists {
                return Err(error);
            }
        }
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Private directory is not a plain directory",
            ));
        }
        security.apply(path)
    }
    fn private_permissions(&self, path: &Path, directory: bool) -> io::Result<()> {
        Security::private(directory)?.apply(path)
    }
    fn open_private(&self, path: &Path, mode: PrivateOpen) -> io::Result<File> {
        let security = Security::private(false)?;
        let disposition = match mode {
            PrivateOpen::New => CREATE_NEW,
            PrivateOpen::Replace | PrivateOpen::Diagnostic => CREATE_ALWAYS,
            PrivateOpen::Lock => OPEN_ALWAYS,
        };
        let handle = Handle::new(unsafe {
            CreateFileW(
                wide(path)?.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &security.attributes(),
                disposition,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        })?;
        let file = unsafe { File::from_raw_handle(handle.0) };
        std::mem::forget(handle);
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Private file is a reparse point",
            ));
        }
        security.apply(path)?;
        Ok(file)
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
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status < 0 {
            return Err(format!("Random number generation failed: {status}"));
        }
        Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
    }
    fn open_editor(&self, path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Editor target is a reparse point",
            ));
        }
        let mut info = unsafe { zeroed::<BY_HANDLE_FILE_INFORMATION>() };
        check(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) })?;
        if info.nNumberOfLinks != 1 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Hardlinked files cannot be edited",
            ));
        }
        Ok(file)
    }
    fn revision(&self, bytes: &[u8], metadata: &fs::Metadata, path: &Path) -> String {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hash);
        identity(path).ok().hash(&mut hash);
        (
            metadata.creation_time(),
            metadata.last_write_time(),
            metadata.file_size(),
            metadata.file_attributes(),
        )
            .hash(&mut hash);
        format!("{:016x}", hash.finish())
    }
    fn editable(&self, metadata: &fs::Metadata) -> bool {
        metadata.is_file()
            && metadata.file_attributes() & (FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_REPARSE_POINT)
                == 0
    }
    fn preserve_attributes(&self, file: &File, metadata: &fs::Metadata) -> Result<(), String> {
        file.set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())
    }
    fn watch(
        &self,
        path: String,
        changed: Box<dyn Fn() + Send>,
    ) -> Result<Box<dyn FileWatch>, String> {
        Ok(Box::new(super::watch::Watch::start(path, changed)?))
    }
}
