#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(target_os = "linux"))]
compile_error!("This prototype currently supports Linux only");

mod containment;
mod instance;
mod session;
mod storage;
mod tab_types;
mod workspace;
use session::{Output, Status, TerminalSession};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{ipc::Channel, Manager, State};
use workspace::{Snapshot, TabConfig, WorkspaceRegistry};

#[derive(Default)]
struct Sessions(Arc<Mutex<WorkspaceRegistry>>);

#[derive(Default)]
struct Lifecycle {
    quitting: AtomicBool,
}

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
async fn connect(
    state: State<'_, Sessions>,
    tab_id: u64,
    cols: u16,
    rows: u16,
    output: Channel<Output>,
    start_if_stopped: bool,
) -> Result<Option<session::Attachment>, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry.lock().map_err(|e| e.to_string())?.connect(
            tab_id,
            cols,
            rows,
            output,
            start_if_stopped,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn start(
    state: State<'_, Sessions>,
    tab_id: u64,
    cols: u16,
    rows: u16,
    output: Channel<Output>,
) -> Result<u64, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = registry.lock().map_err(|e| e.to_string())?;
        guard.start(tab_id, cols, rows, output)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn tab_types(
    app: tauri::AppHandle,
    state: State<Sessions>,
    workspace_id: u64,
) -> Result<Vec<tab_types::TabType>, String> {
    let group = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .workspace_workgroup(workspace_id)?;
    tab_types::load_workgroup(
        &app.path().app_config_dir().map_err(|e| e.to_string())?,
        group,
    )
}

#[tauri::command]
fn workgroup_config(
    app: tauri::AppHandle,
    state: State<Sessions>,
    id: u64,
) -> Result<String, String> {
    state.0.lock().map_err(|e| e.to_string())?.workgroup(id)?;
    let directory = app.path().app_config_dir().map_err(|e| e.to_string())?;
    tab_types::load_workgroup(&directory, id)?;
    Ok(tab_types::workgroup_path(&directory, id)
        .to_string_lossy()
        .into())
}

#[tauri::command]
fn create_workgroup(
    state: State<Sessions>,
    name: String,
    root_directory: String,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .create_workgroup(name, root_directory)
}

#[tauri::command]
fn update_workgroup(
    state: State<Sessions>,
    id: u64,
    name: String,
    root_directory: String,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .update_workgroup(id, name, root_directory)
}

#[tauri::command]
fn fold_workgroup(state: State<Sessions>, id: u64, collapsed: bool) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .fold_workgroup(id, collapsed)
}

#[tauri::command]
fn close_workgroup(state: State<Sessions>, id: u64) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .close_workgroup(id)
}

#[tauri::command]
fn create_tab(
    app: tauri::AppHandle,
    state: State<Sessions>,
    workspace_id: u64,
    type_id: String,
) -> Result<Snapshot, String> {
    let template = tab_types(app, state.clone(), workspace_id)?
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
async fn close_tab(
    state: State<'_, Sessions>,
    id: u64,
    confirmed: bool,
) -> Result<Snapshot, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .close_tab(id, confirmed)
    })
    .await
    .map_err(|e| e.to_string())?
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
async fn close_session(state: State<'_, Sessions>, id: u64, confirmed: bool) -> Result<(), String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = registry.lock().map_err(|e| e.to_string())?;
        guard.close_session(id, confirmed)
    })
    .await
    .map_err(|e| e.to_string())?
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
    workgroup_id: Option<u64>,
) -> Result<Snapshot, String> {
    let mut registry = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(group) = workgroup_id {
        registry.create_in_workgroup(group, name, directory)
    } else {
        registry.create(name, directory)
    }
}

#[tauri::command]
fn update_workspace(
    state: State<Sessions>,
    id: u64,
    name: String,
    directory: String,
    workgroup_id: Option<u64>,
) -> Result<Snapshot, String> {
    let mut registry = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(group) = workgroup_id {
        registry.update_in_workgroup(id, group, name, directory)
    } else {
        registry.update(id, name, directory)
    }
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
async fn close_workspace(
    state: State<'_, Sessions>,
    id: u64,
    confirmed: bool,
) -> Result<Snapshot, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .close(id, confirmed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn shutdown(state: State<'_, Sessions>, confirmed: bool) -> Result<(), String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .shutdown(confirmed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationStatus {
    running_sessions: usize,
    visible: bool,
}

#[tauri::command]
fn application_status(
    app: tauri::AppHandle,
    state: State<Sessions>,
) -> Result<ApplicationStatus, String> {
    let mut registry = state.0.lock().map_err(|e| e.to_string())?;
    let mut running_sessions = 0;
    for session in registry.sessions.values_mut() {
        if matches!(session.status()?, Status::Running) {
            running_sessions += 1;
        }
    }
    Ok(ApplicationStatus {
        running_sessions,
        visible: app
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().unwrap_or(false)),
    })
}

#[tauri::command]
async fn quit(
    app: tauri::AppHandle,
    state: State<'_, Sessions>,
    lifecycle: State<'_, Lifecycle>,
    confirmed: bool,
) -> Result<(), String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .shutdown(confirmed)
    })
    .await
    .map_err(|e| e.to_string())??;
    lifecycle.quitting.store(true, Ordering::Release);
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn defaults() -> Result<(String, String), String> {
    Ok((
        std::env::var("HOME").map_err(|e| e.to_string())?,
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()),
    ))
}

fn main() {
    if containment::run_helper_if_requested() {
        return;
    }
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let context = tauri::generate_context!();
    let directory = instance::data_directory(&context.config().identifier)?;
    let socket = instance::socket_path(&directory)?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("Terminal Workspace\n  (no arguments)  Start or show the application\n  --show          Show a running instance; fail if unavailable");
        return Ok(());
    }
    if !args.is_empty() && args != ["--show"] {
        return Err("Unknown arguments; use --help".into());
    }
    if instance::show(&socket).is_ok() {
        return Ok(());
    }
    if args == ["--show"] {
        return Err("Application unavailable; --show never starts it".into());
    }
    let registry = match WorkspaceRegistry::open(&directory) {
        Ok(registry) => registry,
        Err(error) => {
            // A simultaneous launch may own the lock before its socket is ready.
            for _ in 0..40 {
                if instance::show(&socket).is_ok() {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            return Err(error);
        }
    };
    tauri::Builder::default()
        .manage(Sessions(Arc::new(Mutex::new(registry))))
        .manage(Lifecycle::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            app.manage(
                instance::Instance::start(socket, app.handle().clone())
                    .map_err(std::io::Error::other)?,
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            create_workgroup,
            update_workgroup,
            fold_workgroup,
            close_workgroup,
            workgroup_config,
            application_status,
            quit,
            start,
            connect,
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
        .build(context)
        .map_err(|e| e.to_string())?
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if !app.state::<Lifecycle>().quitting.load(Ordering::Acquire) {
                    api.prevent_exit();
                }
            }
            if let tauri::RunEvent::Exit = event {
                app.state::<instance::Instance>().stop();
                if let Ok(mut session) = app.state::<Sessions>().0.lock() {
                    let _ = session.shutdown(true);
                }
            }
        });
    Ok(())
}
