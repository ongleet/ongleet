use super::protocol::{
    Command, Request, Response, RpcError, REQUEST_LIMIT, RESPONSE_LIMIT, VERSION,
};
use std::{
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::Path,
    time::{Duration, Instant},
};

pub fn verify_peer(stream: &UnixStream) -> Result<(), RpcError> {
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
        return Err(RpcError::new(
            "permission_denied",
            "Socket peer is not the current user",
        ));
    }
    Ok(())
}

// An absolute deadline prevents a client from extending a timeout one byte at a time.
pub fn read_message(
    stream: &mut UnixStream,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<u8>, RpcError> {
    let deadline = Instant::now() + timeout;
    let mut result = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RpcError::new("timeout", "Socket deadline exceeded; a submitted operation may still complete. Check application state before retrying."));
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|e| RpcError::new("operation_failed", e.to_string()))?;
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(result),
            Ok(n) => {
                if result.len() + n > limit { return Err(RpcError::new("message_too_large", "Socket message exceeds its size limit")); }
                result.extend_from_slice(&buffer[..n]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {},
            Err(error) if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => return Err(RpcError::new("timeout", "Socket deadline exceeded; a submitted operation may still complete. Check application state before retrying.")),
            Err(error) => return Err(RpcError::new("operation_failed", error.to_string())),
        }
    }
}

pub fn call(path: &Path, command: Command) -> Result<Response, RpcError> {
    let bytes = serde_json::to_vec(&Request {
        version: VERSION,
        command,
    })
    .map_err(|e| RpcError::new("invalid_argument", e.to_string()))?;
    if bytes.len() > REQUEST_LIMIT {
        return Err(RpcError::new("request_too_large", "Request exceeds 16 KiB"));
    }
    let mut stream = UnixStream::connect(path).map_err(|e| {
        RpcError::new(
            "unavailable",
            format!(
                "Application unavailable: {e}. Start it explicitly with `devcenter app start`."
            ),
        )
    })?;
    verify_peer(&stream)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| RpcError::new("operation_failed", e.to_string()))?;
    stream
        .write_all(&bytes)
        .and_then(|_| stream.shutdown(std::net::Shutdown::Write))
        .map_err(|e| RpcError::new("operation_failed", e.to_string()))?;
    let bytes = read_message(&mut stream, RESPONSE_LIMIT, Duration::from_secs(30))?;
    let response: Response = serde_json::from_slice(&bytes).map_err(|_| RpcError::new("protocol_error", "Invalid application response. Restart the application to load the current socket protocol."))?;
    if response.ok == response.error.is_some() {
        return Err(RpcError::new(
            "protocol_error",
            "Inconsistent application response",
        ));
    }
    Ok(response)
}
