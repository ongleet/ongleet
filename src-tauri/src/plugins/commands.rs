use crate::{desktop_input, plugins, session::Snapshot, AppRegistry};
use tauri::State;

#[tauri::command]
pub async fn associate_pull_request(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    id: u64,
    target: String,
    number: Option<u64>,
) -> Result<Snapshot, String> {
    let association = if target.trim().is_empty() {
        None
    } else {
        Some(plugins::pull_request::Association::parse(
            target.trim(),
            number,
        )?)
    };
    let registry = state.0.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        registry
            .lock()
            .map_err(|e| e.to_string())?
            .associate_pull_request(id, association)
    })
    .await
    .map_err(|e| e.to_string())??;
    use tauri::Emitter;
    let _ = app.emit("session-changed", ());
    Ok(snapshot)
}

#[tauri::command]
pub async fn refresh_pull_request(
    state: State<'_, AppRegistry>,
    id: u64,
) -> Result<serde_json::Value, String> {
    use std::sync::{atomic::Ordering, Arc};
    let registry = state.0.clone();
    let (association, cancel) = {
        let mut guard = registry.lock().map_err(|e| e.to_string())?;
        guard.plugin_tab(id, "pull-request")?;
        if guard.plugin_jobs.len() >= 32 {
            return Err("Too many active plugin requests (limit 32)".into());
        }
        if guard.plugin_jobs.contains_key(&id) {
            return Err("PR refresh is already running".into());
        }
        let association = guard
            .snapshot()
            .sessions
            .iter()
            .find(|s| s.tabs.iter().any(|t| t.id == id))
            .and_then(|s| s.pull_request.clone())
            .ok_or("Associate a GitHub PR with this session first")?;
        let cancel = Arc::new(plugins::Job::default());
        guard.plugin_jobs.insert(id, cancel.clone());
        (association, cancel)
    };
    tauri::async_runtime::spawn_blocking(move || {
        let result = plugins::pull_request::fetch(&association, cancel.cancel.clone());
        cancel.finished.store(true, Ordering::Release);
        if let Ok(mut guard) = registry.lock() {
            if guard
                .plugin_jobs
                .get(&id)
                .is_some_and(|job| Arc::ptr_eq(job, &cancel))
            {
                guard.plugin_jobs.remove(&id);
            }
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_plugin_files(
    state: State<'_, AppRegistry>,
    id: u64,
    folder: Option<String>,
    offset: usize,
    order: Option<plugins::files::SortOrder>,
    show_hidden: Option<bool>,
) -> Result<plugins::files::Listing, String> {
    let registry = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let folder = {
            let guard = registry.lock().map_err(|e| e.to_string())?;
            let tab = guard.plugin_tab(id, "files")?;
            folder
                .or_else(|| tab.config["folder"].as_str().map(String::from))
                .ok_or("File browser folder is unavailable")?
        };
        let listing = plugins::files::list(
            &folder,
            offset,
            order.unwrap_or_default(),
            show_hidden.unwrap_or(false),
        )?;
        let mut guard = registry.lock().map_err(|e| e.to_string())?;
        let current = guard.plugin_tab(id, "files")?;
        if offset == 0 && current.config["folder"].as_str() != Some(&listing.folder) {
            guard.configure_plugin_tab(id, serde_json::json!({"folder": listing.folder}))?;
        }
        Ok(listing)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_plugin_file(
    state: State<'_, AppRegistry>,
    id: u64,
    path: String,
) -> Result<(), String> {
    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .plugin_tab(id, "files")?;
    if !std::path::Path::new(&path).is_absolute() || !std::path::Path::new(&path).is_file() {
        return Err("Choose an existing file with an absolute path".into());
    }
    use gtk::gio::prelude::*;
    let file = gtk::gio::File::for_path(&path);
    desktop_input::open_terminal_link(file.uri().to_string()).await
}
