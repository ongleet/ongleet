use super::super::{ControlConnection, ControlEndpoint, ControlListener, ControlTransport};
use std::{
    fs, io,
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    time::Duration,
};
fn verify_peer(stream: &UnixStream) -> Result<(), String> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut size,
        )
    } != 0
        || credentials.uid != unsafe { libc::geteuid() }
    {
        return Err("Socket peer is not the current user".into());
    }
    Ok(())
}

impl ControlConnection for UnixStream {
    fn verify_owner(&self) -> Result<(), String> {
        verify_peer(self)
    }
    fn read_timeout(&self, timeout: Duration) -> io::Result<()> {
        self.set_read_timeout(Some(timeout))
    }
    fn write_timeout(&self, timeout: Duration) -> io::Result<()> {
        self.set_write_timeout(Some(timeout))
    }
    fn finish_write(&self) -> io::Result<()> {
        self.shutdown(std::net::Shutdown::Write)
    }
}
struct Listener {
    listener: UnixListener,
    path: PathBuf,
}
impl ControlListener for Listener {
    fn accept(&self) -> io::Result<Box<dyn ControlConnection>> {
        self.listener
            .accept()
            .map(|(stream, _)| Box::new(stream) as Box<dyn ControlConnection>)
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
impl ControlTransport for super::Linux {
    fn connect(&self, endpoint: &ControlEndpoint) -> Result<Box<dyn ControlConnection>, String> {
        UnixStream::connect(location(endpoint)?)
            .map(|stream| Box::new(stream) as Box<dyn ControlConnection>)
            .map_err(|e| e.to_string())
    }
    fn listen(&self, endpoint: &ControlEndpoint) -> Result<Box<dyn ControlListener>, String> {
        let path = location(endpoint)?;
        match fs::symlink_metadata(path) {
            Ok(metadata)
                if metadata.file_type().is_socket()
                    && metadata.uid() == unsafe { libc::geteuid() } =>
            {
                fs::remove_file(path).map_err(|e| e.to_string())?
            }
            Ok(_) => return Err("Refusing to replace an unexpected application socket path".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let listener = UnixListener::bind(path).map_err(|e| e.to_string())?;
        let listener = Listener {
            listener,
            path: path.to_owned(),
        };
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        listener
            .listener
            .set_nonblocking(true)
            .map_err(|e| e.to_string())?;
        Ok(Box::new(listener))
    }
}

fn location(endpoint: &ControlEndpoint) -> Result<&Path, String> {
    endpoint
        .0
        .downcast_ref::<PathBuf>()
        .map(|path| path.as_path())
        .ok_or_else(|| "Invalid control endpoint".into())
}
