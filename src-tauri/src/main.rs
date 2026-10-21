#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(not(target_os = "linux"))]
compile_error!("This prototype currently supports Linux only");

mod session;
use session::{Launch, Output, Status, TerminalSession};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use tauri::{ipc::Channel, Manager, State};

#[derive(Default)]
struct Sessions(Mutex<Option<TerminalSession>>);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_session<T>(
    state: &Sessions,
    id: u64,
    operation: impl FnOnce(&mut TerminalSession) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let session = guard
        .as_mut()
        .filter(|s| s.id == id)
        .ok_or("Session is unavailable")?;
    operation(session)
}

#[tauri::command]
fn start(state: State<Sessions>, launch: Launch, output: Channel<Output>) -> Result<u64, String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    if guard.is_some() {
        return Err("Close the existing prototype session first".into());
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    *guard = Some(TerminalSession::spawn(id, launch, output)?);
    Ok(id)
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
    with_session(&state, id, |s| s.status())
}

#[tauri::command]
fn close_session(state: State<Sessions>, id: u64, confirmed: bool) -> Result<(), String> {
    let mut guard = state.0.lock().map_err(|e| e.to_string())?;
    let session = guard
        .as_mut()
        .filter(|s| s.id == id)
        .ok_or("Session is unavailable")?;
    // Even an exited shell may have remaining background jobs.
    if !confirmed {
        return Err("Explicit confirmation is required to terminate session processes".into());
    }
    session.close()?;
    guard.take();
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
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(Sessions::default())
        .invoke_handler(tauri::generate_handler![
            start,
            input,
            acknowledge,
            resize,
            status,
            close_session,
            defaults
        ])
        .build(tauri::generate_context!())
        .expect("Unable to build terminal prototype")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Ok(mut session) = app.state::<Sessions>().0.lock() {
                    session.take();
                }
            }
        });
}
