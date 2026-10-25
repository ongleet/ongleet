#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(target_os = "linux"))]
compile_error!("DevCenter currently supports Linux only");

mod actions;
mod branding;
mod codex;
mod containment;
mod control;
#[path = "control/dispatch.rs"]
mod control_dispatch;
mod core;
use crate::core::desktop as desktop_input;
mod input_diagnostics;
mod instance;
mod plugins;
mod session;
mod storage;
mod terminal_session;
use session::{SessionRegistry, Snapshot, TabConfig};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Manager, State};
use terminal_session::Status;

#[derive(Default)]
struct AppRegistry(Arc<Mutex<SessionRegistry>>);

struct Lifecycle {
    quitting: AtomicBool,
    visible: AtomicBool,
}
impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            quitting: AtomicBool::new(false),
            visible: AtomicBool::new(true),
        }
    }
}

#[tauri::command]
fn actions(state: State<AppRegistry>, session_id: u64) -> Result<Vec<actions::Action>, String> {
    let group = state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .session_project(session_id)?;
    actions::load_project(&control::paths::config_directory()?, group)
}

#[tauri::command]
fn project_config(state: State<AppRegistry>, id: u64) -> Result<String, String> {
    state.0.lock().map_err(|e| e.to_string())?.project(id)?;
    let directory = control::paths::config_directory()?;
    actions::load_project(&directory, id)?;
    Ok(actions::project_path(&directory, id)
        .to_string_lossy()
        .into())
}

#[tauri::command]
fn project_actions(state: State<AppRegistry>, id: u64) -> Result<String, String> {
    state.0.lock().map_err(|e| e.to_string())?.project(id)?;
    actions::read_project_yaml(&control::paths::config_directory()?, id)
}

#[tauri::command]
fn save_project_actions(state: State<AppRegistry>, id: u64, yaml: String) -> Result<(), String> {
    let registry = state.0.lock().map_err(|e| e.to_string())?;
    registry.project(id)?;
    actions::save_project_yaml(&control::paths::config_directory()?, id, &yaml)
}

#[tauri::command]
fn parse_actions(yaml: String) -> Result<actions::Config, String> {
    actions::parse_config(&yaml)
}

#[tauri::command]
fn format_actions(config: actions::Config) -> Result<String, String> {
    actions::format(config)
}

#[tauri::command]
fn create_project(
    state: State<AppRegistry>,
    name: String,
    root_directory: String,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .create_project(name, root_directory)
}

#[tauri::command]
fn update_project(
    state: State<AppRegistry>,
    id: u64,
    name: String,
    root_directory: String,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .update_project(id, name, root_directory)
}

#[tauri::command]
fn fold_project(state: State<AppRegistry>, id: u64, collapsed: bool) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .fold_project(id, collapsed)
}

#[tauri::command]
fn close_project(state: State<AppRegistry>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.close_project(id)
}

#[tauri::command]
fn create_tab(
    state: State<AppRegistry>,
    session_id: u64,
    action_id: String,
) -> Result<Snapshot, String> {
    let template = actions(state.clone(), session_id)?
        .into_iter()
        .find(|t| t.id == action_id)
        .ok_or("Action is unavailable")?;
    if template.kind == "command" {
        return Err("External actions cannot open a tab".into());
    }
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .create_plugin_tab(
            session_id,
            template.name,
            template.kind,
            template.config_version,
            template.config,
        )
}

#[tauri::command]
async fn run_external_action(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    session_id: u64,
    action_id: String,
) -> Result<u32, String> {
    let directory = control::paths::config_directory()?;
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let prepared = registry
            .lock()
            .map_err(|e| e.to_string())?
            .prepare_external(session_id, &action_id, &directory)?;
        actions::external::launch(
            prepared,
            Arc::new(move |exit| {
                use tauri::Emitter;
                let _ = app.emit("external-action-exited", exit);
            }),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn select_tab(state: State<AppRegistry>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.select_tab(id)
}

#[tauri::command]
fn rename_tab(state: State<AppRegistry>, id: u64, name: String) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .rename_tab(id, name)
}

#[tauri::command]
fn configure_tab(
    state: State<AppRegistry>,
    id: u64,
    config: TabConfig,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .configure_tab(id, config)
}

#[tauri::command]
fn reorder_tab(state: State<AppRegistry>, id: u64, position: usize) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .reorder_tab(id, position)
}

#[tauri::command]
async fn close_tab(
    state: State<'_, AppRegistry>,
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
fn open_tab(state: State<AppRegistry>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.open_tab(id)
}

#[tauri::command]
fn archives(state: State<AppRegistry>) -> Result<session::archives::ArchiveList, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.archives())
}
#[tauri::command]
async fn archive_tab(
    state: State<'_, AppRegistry>,
    id: u64,
    confirmed: bool,
) -> Result<Snapshot, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .delete_tab(id, confirmed)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn restore_archive(state: State<'_, AppRegistry>, id: u64) -> Result<Snapshot, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .restore_archive(id)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn purge_archive(
    state: State<'_, AppRegistry>,
    id: u64,
    confirmed: bool,
) -> Result<Snapshot, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .purge_archive(id, confirmed)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn prune_archives(state: State<'_, AppRegistry>, confirmed: bool) -> Result<usize, String> {
    if !confirmed {
        return Err("Explicit confirmation is required to prune archives".into());
    }
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .purge_expired_archives(session::archives::now()?)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn sessions(state: State<AppRegistry>) -> Result<Snapshot, String> {
    Ok(state.0.lock().map_err(|e| e.to_string())?.snapshot())
}

#[tauri::command]
fn create_session(
    state: State<AppRegistry>,
    name: String,
    directory: String,
    project_id: Option<u64>,
) -> Result<Snapshot, String> {
    let directory_config = control::paths::config_directory()?;
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .create_from_first_action(project_id, name, directory, &directory_config)
}

#[tauri::command]
fn update_session(
    state: State<AppRegistry>,
    id: u64,
    name: String,
    directory: String,
    project_id: Option<u64>,
) -> Result<Snapshot, String> {
    let mut registry = state.0.lock().map_err(|e| e.to_string())?;
    if let Some(group) = project_id {
        registry.update_in_project(id, group, name, directory)
    } else {
        registry.update(id, name, directory)
    }
}

#[tauri::command]
fn select_session(state: State<AppRegistry>, id: u64) -> Result<Snapshot, String> {
    state.0.lock().map_err(|e| e.to_string())?.select(id)
}

#[tauri::command]
fn reorder_session(
    state: State<AppRegistry>,
    id: u64,
    position: usize,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .reorder(id, position)
}

#[tauri::command]
fn reorder_project(
    state: State<AppRegistry>,
    id: u64,
    position: usize,
) -> Result<Snapshot, String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .reorder_project(id, position)
}

#[tauri::command]
async fn close_session(
    state: State<'_, AppRegistry>,
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
async fn shutdown(state: State<'_, AppRegistry>, confirmed: bool) -> Result<(), String> {
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
    running_terminals: usize,
    visible: bool,
}

#[tauri::command]
fn application_status(
    app: tauri::AppHandle,
    state: State<AppRegistry>,
) -> Result<ApplicationStatus, String> {
    let mut registry = state.0.lock().map_err(|e| e.to_string())?;
    let mut running_terminals = 0;
    for session in registry.terminals.values_mut() {
        if matches!(session.status()?, Status::Running) {
            running_terminals += 1;
        }
    }
    Ok(ApplicationStatus {
        running_terminals,
        visible: app
            .get_webview_window("main")
            .is_some_and(|window| window.is_visible().unwrap_or(false)),
    })
}

#[tauri::command]
async fn quit(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
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
    if codex::run_if_requested() {
        return;
    }
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
    let directory = instance::data_directory(control::paths::APPLICATION_ID)?;
    let socket = instance::socket_path(&directory)?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("DevCenter\n  (no arguments)  Start or show the application\n  --show          Show a running instance; fail if unavailable\n  --migrate       Migrate config/data and database schema without opening a window");
        return Ok(());
    }
    if args == ["--migrate"] {
        branding::migrate(&directory, &control::paths::config_directory()?)?;
        let _store = storage::MetadataStore::open(&directory)?;
        actions::migrate_projects(&control::paths::config_directory()?)?;
        println!(
            "DevCenter config: {}\nDevCenter database: {}",
            control::paths::config_directory()?.display(),
            directory.join("workspaces.sqlite3").display()
        );
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
    branding::migrate(&directory, &control::paths::config_directory()?)?;
    let registry = match SessionRegistry::open(&directory) {
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
    actions::migrate_projects(&control::paths::config_directory()?)?;
    tauri::Builder::default()
        .manage(AppRegistry(Arc::new(Mutex::new(registry))))
        .manage(Lifecycle::default())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            input_diagnostics::install(app)?;
            app.state::<AppRegistry>()
                .0
                .lock()
                .map_err(|e| std::io::Error::other(e.to_string()))?
                .set_actions_directory(control::paths::config_directory()?);
            app.manage(
                instance::Instance::start(socket, app.handle().clone())
                    .map_err(std::io::Error::other)?,
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if window.hide().is_ok() {
                    window
                        .state::<Lifecycle>()
                        .visible
                        .store(false, Ordering::Release);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            archives,
            archive_tab,
            restore_archive,
            purge_archive,
            prune_archives,
            create_project,
            update_project,
            fold_project,
            close_project,
            project_config,
            project_actions,
            parse_actions,
            format_actions,
            save_project_actions,
            application_status,
            quit,
            defaults,
            sessions,
            create_session,
            update_session,
            select_session,
            reorder_session,
            reorder_project,
            desktop_input::terminal_clipboard,
            desktop_input::open_terminal_link,
            close_session,
            core::commands::sdk_attach,
            core::commands::sdk_detach,
            core::commands::sdk_request,
            core::commands::sdk_pty,
            core::commands::sdk_pty_connect,
            create_tab,
            run_external_action,
            actions,
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
                if let Ok(mut session) = app.state::<AppRegistry>().0.lock() {
                    let _ = session.shutdown(true);
                }
            }
        });
    Ok(())
}
