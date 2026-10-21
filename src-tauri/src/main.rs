#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(target_os = "linux"))]
compile_error!("This prototype currently supports Linux only");

mod session;
mod storage;
mod tab_types;
mod workspace;
use session::{Output, Status, TerminalSession};
use std::sync::Mutex;
use tauri::{ipc::Channel, Manager, State};
use workspace::{Snapshot, TabConfig, WorkspaceRegistry};

#[derive(Default)]
struct Sessions(Mutex<WorkspaceRegistry>);

fn with_session<T>(
    state: &Sessions,
    id: u64,
    operation: impl FnOnce(&mut TerminalSession) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let session = guard
        .sessions
        .values_mut()
        .find(|s| s.id == id)
        .ok_or("Session is unavailable")?;
    operation(session)
}

#[tauri::command]
fn start(
    state: State<Sessions>,
    tab_id: u64,
    cols: u16,
    rows: u16,
    output: Channel<Output>,
) -> Result<u64, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    guard.start(tab_id, cols, rows, output)
}

#[tauri::command]
fn tab_types(app: tauri::AppHandle) -> Result<Vec<tab_types::TabType>, String> {
    tab_types::load(&app.path().app_config_dir().map_err(|e| e.to_string())?)
}

#[tauri::command]
fn create_tab(
    app: tauri::AppHandle,
    state: State<Sessions>,
    workspace_id: u64,
    type_id: String,
) -> Result<Snapshot, String> {
    let template = tab_types(app)?
        .into_iter()
        .find(|t| t.id == type_id)
        .ok_or("Tab type is unavailable")?;
    state.0.lock().map_err(|e| e.to_string())?.create_typed_tab(
        workspace_id,
        template.name,
        template.config,
    )
}

#[tauri::command]
fn select_tab(state: State<Sessions>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.select_tab(id)
}

#[tauri::command]
fn rename_tab(state: State<Sessions>, id: u64, name: String) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .rename_tab(id, name)
}

#[tauri::command]
fn configure_tab(state: State<Sessions>, id: u64, config: TabConfig) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .configure_tab(id, config)
}

#[tauri::command]
fn reorder_tab(state: State<Sessions>, id: u64, position: usize) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .reorder_tab(id, position)
}

#[tauri::command]
fn close_tab(state: State<Sessions>, id: u64, confirmed: bool) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .close_tab(id, confirmed)
}

#[tauri::command]
async fn input(state: State<'_, Sessions>, id: u64, bytes: Vec<u8>) -> Result<(), String> {
    let result = with_session(&state, id, |s| s.queue_input(bytes))?;
    // A blocked PTY writer must never hold the session lock or WebView thread:
    // ACK, resize and confirmed close still need to make progress.
    tauri::async_runtime::spawn_blocking(move || result.recv().map_err(|e| e.to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn acknowledge(state: State<Sessions>, id: u64, seq: u64) -> Result<(), String> {
    with_session(&state, id, |s| s.acknowledge(seq))
}

#[tauri::command]
fn resize(state: State<Sessions>, id: u64, cols: u16, rows: u16) -> Result<(), String> {
    with_session(&state, id, |s| s.resize(cols, rows))
}

#[tauri::command]
fn status(state: State<Sessions>, id: u64) -> Result<Status, String> {
    state.0.lock().map_err(|e| e.to_string())?.status(id)
}

#[tauri::command]
fn open_tab(state: State<Sessions>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.open_tab(id)
}

#[tauri::command]
fn close_session(state: State<Sessions>, id: u64, confirmed: bool) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    guard.close_session(id, confirmed)
}

#[tauri::command]
fn workspaces(state: State<Sessions>) -> Result<Snapshot, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.snapshot())
}

#[tauri::command]
fn create_workspace(
    state: State<Sessions>,
    name: String,
    directory: String,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .create(name, directory)
}

#[tauri::command]
fn update_workspace(
    state: State<Sessions>,
    id: u64,
    name: String,
    directory: String,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .update(id, name, directory)
}

#[tauri::command]
fn select_workspace(state: State<Sessions>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.select(id)
}

#[tauri::command]
fn reorder_workspace(state: State<Sessions>, id: u64, position: usize) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .reorder(id, position)
}

#[tauri::command]
fn close_workspace(state: State<Sessions>, id: u64, confirmed: bool) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .close(id, confirmed)
}

#[tauri::command]
fn shutdown(state: State<Sessions>, confirmed: bool) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .shutdown(confirmed)
}

#[tauri::command]
fn defaults() -> Result<(String, String), String> {
    Ok((
        std::env::var("HOME").map_err(|e| e.to_string())?,
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
    ))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let registry = WorkspaceRegistry::open(&directory).map_err(std::io::Error::other)?;
            app.manage(Sessions(Mutex::new(registry)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start,
            input,
            acknowledge,
            resize,
            status,
            close_session,
            defaults,
            workspaces,
            create_workspace,
            update_workspace,
            select_workspace,
            reorder_workspace,
            close_workspace,
            create_tab,
            tab_types,
            select_tab,
            rename_tab,
            configure_tab,
            reorder_tab,
            close_tab,
            open_tab,
            shutdown
        ])
        .build(tauri::generate_context!())
        .expect("Unable to build terminal prototype")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Ok(mut session) = app.state::<Sessions>().0.lock() {
                    let _ = session.shutdown(true);
                }
            }
        });
}
