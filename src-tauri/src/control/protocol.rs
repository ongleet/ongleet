use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VERSION: u32 = 2;
pub const REQUEST_LIMIT: usize = 128 * 1024;
pub const RESPONSE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct Request {
    pub version: u32,
    #[serde(flatten)]
    pub command: Command,
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut fields = serde_json::Map::<String, Value>::deserialize(deserializer)?;
        let version = fields
            .remove("version")
            .ok_or_else(|| D::Error::missing_field("version"))?;
        let version = serde_json::from_value(version).map_err(D::Error::custom)?;
        let command: Command =
            serde_json::from_value(Value::Object(fields.clone())).map_err(D::Error::custom)?;
        // Serde's flattened internally-tagged unit variants otherwise ignore extra fields.
        let allowed = serde_json::to_value(&command).map_err(D::Error::custom)?;
        if fields.keys().any(|key| allowed.get(key).is_none()) {
            return Err(D::Error::custom("Unknown command field"));
        }
        Ok(Self { version, command })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Show,
    Status,
    Quit {
        #[serde(default)]
        confirmed: bool,
    },
    ArchiveList,
    ArchiveRestore {
        id: u64,
    },
    ArchivePurge {
        id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    ArchivePrune {
        #[serde(default)]
        confirmed: bool,
    },
    SessionList,
    SessionConfig {
        id: u64,
    },
    SessionConfigure {
        id: u64,
        name: Option<String>,
        directory: Option<String>,
        project_id: Option<u64>,
    },
    SessionPullRequest {
        id: u64,
        target: String,
        number: Option<u64>,
    },
    SessionCreate {
        project_id: u64,
        name: String,
        #[serde(default)]
        directory: String,
    },
    SessionRename {
        id: u64,
        name: String,
    },
    SessionDirectory {
        id: u64,
        directory: String,
    },
    SessionMove {
        id: u64,
        project_id: u64,
    },
    SessionReorder {
        id: u64,
        position: usize,
    },
    SessionSelect {
        id: u64,
    },
    SessionClose {
        id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    TabTypes,
    TabActions {
        session_id: u64,
    },
    TabList {
        session_id: u64,
    },
    TabCreate {
        session_id: u64,
        action_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config: Option<Value>,
    },
    TabSelect {
        id: u64,
    },
    TabRename {
        id: u64,
        name: String,
    },
    TabReorder {
        id: u64,
        position: usize,
    },
    TabOpen {
        id: u64,
    },
    TabClose {
        id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    TabDelete {
        id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    TabConfigure {
        id: u64,
        config: Value,
    },
    ProjectList,
    ProjectConfig {
        id: u64,
    },
    ProjectConfigure {
        id: u64,
        yaml: String,
    },
    ProjectCreate {
        name: String,
        root: String,
    },
    ProjectRename {
        id: u64,
        name: String,
    },
    ProjectDirectory {
        id: u64,
        directory: String,
    },
    ProjectFold {
        id: u64,
        collapsed: bool,
    },
    ProjectClose {
        id: u64,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}
impl Response {
    pub fn success(data: Option<Value>) -> Self {
        Self {
            ok: true,
            data,
            error: None,
        }
    }
    pub fn failure(error: RpcError) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}
impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
    #[allow(dead_code)] // Used by the separate CLI binary.
    pub fn exit_code(&self) -> i32 {
        match self.code.as_str() {
            "invalid_argument" | "invalid_request" => 2,
            "unavailable" => 3,
            "not_found" => 4,
            "confirmation_required" => 5,
            "permission_denied" => 7,
            "protocol_error" | "request_too_large" | "message_too_large" | "response_too_large"
            | "timeout" => 8,
            _ => 6,
        }
    }
}

// Stop serialization at the wire budget instead of allocating an oversized frame.
pub fn encode_response(response: &Response) -> Vec<u8> {
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len() + bytes.len() >= RESPONSE_LIMIT {
                return Err(std::io::Error::other("Response limit exceeded"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Bounded(Vec::new());
    if serde_json::to_writer(&mut buffer, response).is_err() {
        buffer.0 = serde_json::to_vec(&Response::failure(RpcError::new(
            "response_too_large",
            "Response exceeds 1 MiB",
        )))
        .unwrap();
    }
    buffer.0.push(b'\n');
    buffer.0
}
