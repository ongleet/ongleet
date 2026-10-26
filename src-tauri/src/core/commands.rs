//! Native adapters for the repository-local SDK.
use crate::{session::Namespace, AppRegistry};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

#[derive(Serialize)]
pub struct SdkError {
    code: &'static str,
    message: String,
}
impl From<String> for SdkError {
    fn from(message: String) -> Self {
        let code = if message.contains("revision_conflict")
            || message.starts_with("File changed on disk")
        {
            "revision_conflict"
        } else if message.contains("already running") {
            "busy"
        } else if message.contains("capability") {
            "capability_unavailable"
        } else if message.contains("cancelled") {
            "cancelled"
        } else if message.contains("timed out") {
            "timeout"
        } else if message.contains("exceeds") || message.contains("limit") {
            "limit_exceeded"
        } else if message.contains("unavailable") {
            "unavailable"
        } else {
            "operation_failed"
        };
        Self { code, message }
    }
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    List {
        folder: String,
        offset: usize,
        order: super::fs::SortOrder,
        #[serde(rename = "showHidden")]
        show_hidden: bool,
        query: String,
    },
    OpenFile {
        path: String,
    },
    WatchText {
        path: String,
        token: String,
    },
    UnwatchText {
        token: String,
    },
    ReadText {
        path: String,
    },
    SaveText {
        path: String,
        text: String,
        revision: String,
    },
    AddSessionTag {
        tag: crate::session::SessionTag,
    },
    RemoveSessionTag {
        name: String,
    },
    SessionTags,
    Configure {
        patch: serde_json::Value,
    },
    ReadData {
        scope: Scope,
    },
    ReplaceData {
        scope: Scope,
        revision: u64,
        version: u32,
        value: serde_json::Value,
    },
    Process {
        request: super::process::Request,
    },
    Cancel,
    EnvironmentGet {
        name: String,
    },
    CredentialGet {
        key: String,
    },
    CredentialSet {
        key: String,
        value: String,
    },
    CredentialDelete {
        key: String,
    },
    Http {
        token: String,
        request: super::http::Request,
    },
    HttpCancel {
        token: String,
    },
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    Session,
    Tab,
}

#[tauri::command]
pub async fn sdk_attach(
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
) -> Result<(), SdkError> {
    if attachment.is_empty() || attachment.len() > 128 {
        return Err("Invalid attachment".to_string().into());
    }
    let mut guard = state.0.lock().map_err(|e| SdkError::from(e.to_string()))?;
    guard.open_plugin_tab(id).map_err(SdkError::from)?;
    guard.cancel_plugin_work(id).map_err(SdkError::from)?;
    guard.sdk_attachments.insert(id, attachment);
    Ok(())
}
#[tauri::command]
pub async fn sdk_detach(
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
) -> Result<(), SdkError> {
    let mut guard = state.0.lock().map_err(|e| SdkError::from(e.to_string()))?;
    if guard.sdk_attachments.get(&id) == Some(&attachment) {
        guard.cancel_plugin_work(id).map_err(SdkError::from)?;
        guard.sdk_attachments.remove(&id);
    }
    Ok(())
}
#[tauri::command]
pub async fn sdk_request(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    request: Request,
) -> Result<serde_json::Value, SdkError> {
    let registry = state.0.clone();
    let (session_id, plugin) = {
        let guard = registry.lock().map_err(|e| SdkError::from(e.to_string()))?;
        if guard.sdk_attachments.get(&id) != Some(&attachment) {
            return Err("Frontend attachment unavailable".to_string().into());
        }
        let tab = guard.open_plugin_tab(id).map_err(SdkError::from)?;
        let capability = match &request {
            Request::List { .. }
            | Request::ReadText { .. }
            | Request::SaveText { .. }
            | Request::WatchText { .. }
            | Request::UnwatchText { .. } => "fs",
            Request::Process { .. } | Request::Cancel => "process",
            Request::OpenFile { .. } => "desktop",
            Request::EnvironmentGet { .. } => "env",
            Request::CredentialGet { .. }
            | Request::CredentialSet { .. }
            | Request::CredentialDelete { .. } => "credentials",
            Request::Http { .. } | Request::HttpCancel { .. } => "http",
            _ => "storage",
        };
        if !crate::plugins::manifest(&tab.kind).is_some_and(|m| {
            m["capabilities"]
                .as_array()
                .is_some_and(|c| c.iter().any(|v| v == capability))
        }) {
            return Err("Required core capability unavailable".to_string().into());
        }
        let session = guard
            .snapshot()
            .sessions
            .into_iter()
            .find(|s| s.tabs.iter().any(|t| t.id == id))
            .ok_or_else(|| SdkError::from("Session unavailable".to_string()))?;
        (session.id, tab.kind.clone())
    };
    match &request {
        Request::EnvironmentGet { name } => {
            return super::environment::get(name).map_err(SdkError::from);
        }
        Request::CredentialGet { key }
        | Request::CredentialSet { key, .. }
        | Request::CredentialDelete { key } => {
            let operation = match &request {
                Request::CredentialSet { value, .. } => {
                    super::credentials::Operation::Set(value.clone())
                }
                Request::CredentialDelete { .. } => super::credentials::Operation::Delete,
                _ => super::credentials::Operation::Get,
            };
            let key = key.clone();
            return tokio::time::timeout(
                std::time::Duration::from_secs(20),
                super::credentials::execute(&plugin, &key, operation),
            )
            .await
            .map_err(|_| SdkError::from("Credential storage unavailable: timed out".to_string()))?
            .map_err(SdkError::from);
        }
        Request::HttpCancel { token } => {
            let guard = registry.lock().map_err(|e| SdkError::from(e.to_string()))?;
            if let Some(job) = guard.http_jobs.get(&(id, token.clone())) {
                job.cancel.store(true, std::sync::atomic::Ordering::Release);
            }
            return Ok(serde_json::Value::Null);
        }
        Request::Http { token, request } => {
            if token.is_empty() || token.len() > 128 {
                return Err("Invalid HTTP token".to_string().into());
            }
            let key = (id, token.clone());
            let job = {
                let mut guard = registry.lock().map_err(|e| SdkError::from(e.to_string()))?;
                if guard.sdk_attachments.get(&id) != Some(&attachment)
                    || guard.open_plugin_tab(id).is_err()
                {
                    return Err("Frontend attachment unavailable".to_string().into());
                }
                if guard.http_jobs.len() >= 32
                    || guard.http_jobs.keys().filter(|(tab, _)| *tab == id).count() >= 4
                {
                    return Err("HTTP concurrency limit exceeded".to_string().into());
                }
                if guard.http_jobs.contains_key(&key) {
                    return Err("HTTP request already running".to_string().into());
                }
                let job = std::sync::Arc::new(crate::plugins::Job::default());
                guard.http_jobs.insert(key.clone(), job.clone());
                job
            };
            let result = super::http::execute(request, job.clone()).await;
            job.finished
                .store(true, std::sync::atomic::Ordering::Release);
            registry
                .lock()
                .map_err(|e| SdkError::from(e.to_string()))?
                .http_jobs
                .remove(&key);
            return result.map_err(SdkError::from);
        }
        _ => {}
    }
    if let Request::OpenFile { path } = &request {
        if !std::path::Path::new(path).is_absolute() || !std::path::Path::new(path).is_file() {
            return Err("Choose an existing file with an absolute path"
                .to_string()
                .into());
        }
        use gtk::gio::prelude::*;
        let uri = gtk::gio::File::for_path(path).uri().to_string();
        return super::desktop::open_terminal_link(uri)
            .await
            .map(|_| serde_json::Value::Null)
            .map_err(SdkError::from);
    }
    let changed = matches!(
        &request,
        Request::Configure { .. }
            | Request::ReplaceData { .. }
            | Request::AddSessionTag { .. }
            | Request::RemoveSessionTag { .. }
    );
    // Register jobs before releasing ownership; close/detach can then cancel them.
    let job = if matches!(&request, Request::Process { .. }) {
        let mut guard = registry.lock().map_err(|e| SdkError::from(e.to_string()))?;
        if guard.sdk_attachments.get(&id) != Some(&attachment) || guard.open_plugin_tab(id).is_err()
        {
            return Err("Frontend attachment unavailable".to_string().into());
        }
        if guard.plugin_jobs.len() >= 32 {
            return Err("Active job limit 32".to_string().into());
        }
        if guard.plugin_jobs.contains_key(&id) {
            return Err("A process request is already running".to_string().into());
        }
        let job = std::sync::Arc::new(crate::plugins::Job::default());
        guard.plugin_jobs.insert(id, job.clone());
        Some(job)
    } else {
        None
    };
    let watch_app = app.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
            if let Request::Process { request } = request {
                let job = job.unwrap();
                let result = super::process::execute(request, job.cancel.clone());
                job.finished
                    .store(true, std::sync::atomic::Ordering::Release);
                if let Ok(mut guard) = registry.lock() {
                    if guard
                        .plugin_jobs
                        .get(&id)
                        .is_some_and(|current| std::sync::Arc::ptr_eq(current, &job))
                    {
                        guard.plugin_jobs.remove(&id);
                    }
                }
                return serde_json::to_value(result?).map_err(|e| e.to_string());
            }
            // Keep metadata operations atomic with attachment and tab validation.
            let mut guard = registry.lock().map_err(|e| e.to_string())?;
            if guard.sdk_attachments.get(&id) != Some(&attachment) {
                return Err("Frontend attachment unavailable".into());
            }
            guard.open_plugin_tab(id)?;
            let json = match request {
                Request::EnvironmentGet { .. }
                | Request::CredentialGet { .. }
                | Request::CredentialSet { .. }
                | Request::CredentialDelete { .. }
                | Request::Http { .. }
                | Request::HttpCancel { .. } => unreachable!(),
                Request::List {
                    folder,
                    offset,
                    order,
                    show_hidden,
                    query,
                } => serde_json::to_value(super::fs::list(
                    &folder,
                    offset,
                    order,
                    show_hidden,
                    &query,
                )?),
                Request::WatchText { path, token } => {
                    if token.is_empty() || token.len() > 128 {
                        return Err("Invalid watch token".into());
                    }
                    if !guard.file_watches.contains_key(&id) && guard.file_watches.len() >= 128 {
                        return Err("File watch limit 128".into());
                    }
                    let event = format!("fs-changed-{id}-{attachment}");
                    let notification = token.clone();
                    let watch = super::fs::watch::Watch::start(path, token, move || {
                        let _ = watch_app.emit(&event, &notification);
                    })?;
                    guard.file_watches.insert(id, watch);
                    Ok(serde_json::Value::Null)
                }
                Request::UnwatchText { token } => {
                    if guard
                        .file_watches
                        .get(&id)
                        .is_some_and(|w| w.token == token)
                    {
                        guard.file_watches.remove(&id);
                    }
                    Ok(serde_json::Value::Null)
                }
                Request::ReadText { path } => {
                    if let Some(watch) = guard.file_watches.get(&id).filter(|w| w.path == path) {
                        watch.acknowledge();
                    }
                    serde_json::to_value(super::fs::editor::load(&path)?)
                }
                Request::SaveText {
                    path,
                    text,
                    revision,
                } => serde_json::to_value(super::fs::editor::save(&path, &text, &revision)?),
                Request::AddSessionTag { tag } => {
                    let snapshot = guard.add_session_tag(session_id, tag)?;
                    serde_json::to_value(
                        snapshot
                            .sessions
                            .iter()
                            .find(|s| s.id == session_id)
                            .ok_or("Session unavailable")?
                            .session_tags(),
                    )
                }
                Request::RemoveSessionTag { name } => {
                    let snapshot = guard.remove_session_tag(session_id, name)?;
                    serde_json::to_value(
                        snapshot
                            .sessions
                            .iter()
                            .find(|s| s.id == session_id)
                            .ok_or("Session unavailable")?
                            .session_tags(),
                    )
                }
                Request::SessionTags => {
                    let snapshot = guard.snapshot();
                    let session = snapshot
                        .sessions
                        .iter()
                        .find(|s| s.id == session_id)
                        .ok_or("Session unavailable")?;
                    serde_json::to_value(session.session_tags())
                }
                Request::Configure { patch } => {
                    guard.patch_plugin_config(id, patch)?;
                    Ok(serde_json::Value::Null)
                }
                Request::ReadData { scope } => {
                    let snapshot = guard.snapshot();
                    let session = snapshot
                        .sessions
                        .iter()
                        .find(|s| s.id == session_id)
                        .ok_or("Session unavailable")?;
                    let data = match scope {
                        Scope::Session => &session.plugin_data,
                        Scope::Tab => {
                            &session
                                .tabs
                                .iter()
                                .find(|t| t.id == id)
                                .ok_or("Tab unavailable")?
                                .plugin_data
                        }
                    };
                    serde_json::to_value(data.get(&plugin))
                }
                Request::ReplaceData {
                    scope,
                    revision,
                    version,
                    value,
                } => {
                    if matches!(scope, Scope::Session) {
                        let snapshot = guard.snapshot();
                        let session = snapshot
                            .sessions
                            .iter()
                            .find(|s| s.id == session_id)
                            .ok_or("Session unavailable")?;
                        if session
                            .plugin_data
                            .get(&plugin)
                            .map_or(0, |data| data.revision)
                            != revision
                        {
                            return Err(
                                "revision_conflict: Plugin data changed; reload before saving"
                                    .into(),
                            );
                        }
                        let mut replacement = session.plugin_data.clone();
                        replacement.insert(
                            plugin.clone(),
                            Namespace {
                                version,
                                revision: revision.checked_add(1).ok_or("Revision overflow")?,
                                value: value.clone(),
                            },
                        );
                        if version == 0 {
                            return Err("Invalid namespace version".into());
                        }
                        if serde_json::to_vec(&replacement)
                            .map_err(|e| e.to_string())?
                            .len()
                            > 65_536
                        {
                            return Err("Plugin data exceeds owner's 64 KiB limit".into());
                        }
                        let tabs = guard
                            .snapshot()
                            .sessions
                            .iter()
                            .find(|s| s.id == session_id)
                            .unwrap()
                            .tabs
                            .iter()
                            .filter(|t| t.kind == plugin)
                            .map(|t| t.id)
                            .collect::<Vec<_>>();
                        guard.cancel_plugin_jobs(&tabs)?;
                    }
                    let namespace: Namespace = guard.replace_plugin_data(
                        session_id,
                        if matches!(scope, Scope::Tab) {
                            Some(id)
                        } else {
                            None
                        },
                        &plugin,
                        revision,
                        version,
                        value,
                    )?;
                    serde_json::to_value(namespace)
                }
                Request::Cancel => {
                    guard.cancel_plugin_work(id)?;
                    Ok(serde_json::Value::Null)
                }
                Request::Process { .. } | Request::OpenFile { .. } => unreachable!(),
            };
            json.map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| SdkError::from(e.to_string()))?
        .map_err(SdkError::from)?;
    if changed {
        let _ = app.emit("session-changed", ());
    }
    Ok(result)
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "camelCase", deny_unknown_fields)]
pub enum PtyRequest {
    Connect {
        cols: u16,
        rows: u16,
        #[serde(rename = "startIfStopped")]
        start_if_stopped: bool,
    },
    Acknowledge {
        #[serde(rename = "runtimeId")]
        runtime_id: u64,
        seq: u64,
    },
    Input {
        #[serde(rename = "runtimeId")]
        runtime_id: u64,
        bytes: Vec<u8>,
    },
    Resize {
        #[serde(rename = "runtimeId")]
        runtime_id: u64,
        cols: u16,
        rows: u16,
    },
    Status {
        #[serde(rename = "runtimeId")]
        runtime_id: u64,
    },
    History,
    SaveHistory {
        #[serde(rename = "runtimeId")]
        runtime_id: u64,
        text: String,
    },
    Close {
        #[serde(rename = "runtimeId")]
        runtime_id: u64,
    },
}
#[tauri::command]
pub async fn sdk_pty(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    request: PtyRequest,
) -> Result<serde_json::Value, SdkError> {
    pty_operation(app, state, id, attachment, request, None).await
}
#[tauri::command]
pub async fn sdk_pty_connect(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    cols: u16,
    rows: u16,
    start_if_stopped: bool,
    output: tauri::ipc::Channel<crate::terminal_session::Output>,
) -> Result<serde_json::Value, SdkError> {
    pty_operation(
        app,
        state,
        id,
        attachment,
        PtyRequest::Connect {
            cols,
            rows,
            start_if_stopped,
        },
        Some(output),
    )
    .await
}
async fn pty_operation(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    request: PtyRequest,
    output: Option<tauri::ipc::Channel<crate::terminal_session::Output>>,
) -> Result<serde_json::Value, SdkError> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
        let mut guard = registry.lock().map_err(|e| e.to_string())?;
        if guard.sdk_attachments.get(&id) != Some(&attachment) {
            return Err("Frontend attachment unavailable".into());
        }
        guard.runtime_tab(id)?;
        let runtime_id = match &request {
            PtyRequest::Acknowledge { runtime_id, .. }
            | PtyRequest::Input { runtime_id, .. }
            | PtyRequest::Resize { runtime_id, .. }
            | PtyRequest::Status { runtime_id }
            | PtyRequest::SaveHistory { runtime_id, .. }
            | PtyRequest::Close { runtime_id } => Some(*runtime_id),
            _ => None,
        };
        if let Some(runtime_id) = runtime_id {
            if guard.terminals.get(&id).map(|t| t.id) != Some(runtime_id) {
                return Err("PTY belongs to an unavailable tab/runtime".into());
            }
        }
        let previous_tags = if matches!(
            &request,
            PtyRequest::Status { .. } | PtyRequest::Close { .. }
        ) {
            Some(guard.tags_for_tab(id)?)
        } else {
            None
        };
        let json = match request {
            PtyRequest::Connect {
                cols,
                rows,
                start_if_stopped,
            } => serde_json::to_value(guard.connect(
                id,
                cols,
                rows,
                output.ok_or("Output channel required")?,
                start_if_stopped,
            )?),
            PtyRequest::History => serde_json::to_value(guard.terminal_history(id)?),
            PtyRequest::SaveHistory { runtime_id, text } => {
                guard.save_terminal_history(id, runtime_id, text)?;
                Ok(serde_json::Value::Null)
            }
            PtyRequest::Status { runtime_id } => serde_json::to_value(guard.status(runtime_id)?),
            PtyRequest::Close { runtime_id } => {
                guard.close_terminal_session(runtime_id, true)?;
                Ok(serde_json::Value::Null)
            }
            PtyRequest::Acknowledge { seq, .. } => {
                guard.terminals.get_mut(&id).unwrap().acknowledge(seq)?;
                Ok(serde_json::Value::Null)
            }
            PtyRequest::Resize { cols, rows, .. } => {
                guard.terminals.get_mut(&id).unwrap().resize(cols, rows)?;
                Ok(serde_json::Value::Null)
            }
            PtyRequest::Input { bytes, .. } => {
                let result = guard.terminals.get_mut(&id).unwrap().queue_input(bytes)?;
                drop(guard);
                // A blocked writer must not block ACK, close or registry operations.
                result.recv().map_err(|e| e.to_string())??;
                return Ok(serde_json::Value::Null);
            }
        };
        if let Some(previous) = previous_tags {
            if guard
                .tags_for_tab(id)
                .is_ok_and(|current| current != previous)
            {
                let _ = app.emit("session-changed", ());
            }
        }
        json.map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| SdkError::from(e.to_string()))?
    .map_err(SdkError::from)
}
