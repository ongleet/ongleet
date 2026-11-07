use super::protocol::{
    Command, Request, Response, RpcError, REQUEST_LIMIT, RESPONSE_LIMIT, VERSION,
};
use std::{
    io::Write,
    time::{Duration, Instant},
};

// An absolute deadline prevents a client from extending a timeout one byte at a time.
pub fn read_message(
    stream: &mut dyn crate::system::ControlConnection,
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
            .read_timeout(remaining)
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

pub fn call(
    transport: &dyn crate::system::ControlTransport,
    path: &crate::system::ControlEndpoint,
    command: Command,
) -> Result<Response, RpcError> {
    call_with_timeout(transport, path, command, Duration::from_secs(30))
}

pub fn call_with_timeout(
    transport: &dyn crate::system::ControlTransport,
    path: &crate::system::ControlEndpoint,
    command: Command,
    timeout: Duration,
) -> Result<Response, RpcError> {
    let bytes = serde_json::to_vec(&Request {
        version: VERSION,
        command,
    })
    .map_err(|e| RpcError::new("invalid_argument", e.to_string()))?;
    if bytes.len() > REQUEST_LIMIT {
        return Err(RpcError::new(
            "request_too_large",
            "Request exceeds 128 KiB",
        ));
    }
    let mut stream = transport.connect(path).map_err(|e| {
        RpcError::new(
            "unavailable",
            format!("Application unavailable: {e}. Start it explicitly with `ongleet app start`."),
        )
    })?;
    stream
        .verify_owner()
        .map_err(|e| RpcError::new("permission_denied", e))?;
    stream
        .write_timeout(timeout.min(Duration::from_secs(2)))
        .map_err(|e| RpcError::new("operation_failed", e.to_string()))?;
    stream
        .write_all(&bytes)
        .and_then(|_| stream.finish_write())
        .map_err(|e| RpcError::new("operation_failed", e.to_string()))?;
    let bytes = read_message(stream.as_mut(), RESPONSE_LIMIT, timeout)?;
    let response: Response = serde_json::from_slice(&bytes).map_err(|_| RpcError::new("protocol_error", "Invalid application response. Restart the application to load the current socket protocol."))?;
    if response.ok == response.error.is_some() {
        return Err(RpcError::new(
            "protocol_error",
            "Inconsistent application response",
        ));
    }
    Ok(response)
}
