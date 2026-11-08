use super::super::{
    ClipboardContent, DesktopService, Geometry, NativeDiagnostic, NativeInputDiagnostic,
    PlatformCapabilities, SystemFuture,
};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};
impl DesktopService for super::Macos {
    fn configure_builder(&self, builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
        let diagnostics = self.diagnostics.clone();
        builder.on_web_content_process_terminate(move |_| {
            if let Some(record) = diagnostics.lock().unwrap().as_ref() {
                record(NativeDiagnostic::Terminated(
                    "WKWebView content process terminated".into(),
                ));
            }
        })
    }
    fn reopen_requested(&self, event: &tauri::RunEvent) -> bool {
        matches!(event, tauri::RunEvent::Reopen { .. })
    }
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            command_shortcuts: true,
            primary_selection: "internal",
        }
    }
    fn initialize(&self) {}
    fn show(&self, w: &tauri::WebviewWindow) -> Result<(), String> {
        w.show().map_err(|e| e.to_string())
    }
    fn hide(&self, w: &tauri::WebviewWindow) -> Result<(), String> {
        w.hide().map_err(|e| e.to_string())
    }
    fn activate(&self, w: &tauri::WebviewWindow) -> Result<(), String> {
        w.show()
            .and_then(|_| w.unminimize())
            .and_then(|_| w.set_focus())
            .map_err(|e| e.to_string())
    }
    fn install_menu(&self, app: &tauri::App) -> Result<(), String> {
        use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
        let quit = MenuItem::with_id(app, "ongleet-quit", "Quit Ongleet", true, Some("Cmd+Q"))
            .map_err(|e| e.to_string())?;
        let hide = PredefinedMenuItem::hide(app, None).map_err(|e| e.to_string())?;
        let hide_others = PredefinedMenuItem::hide_others(app, None).map_err(|e| e.to_string())?;
        let show_all = PredefinedMenuItem::show_all(app, None).map_err(|e| e.to_string())?;
        let submenu = Submenu::with_items(
            app,
            "Ongleet",
            true,
            &[&hide, &hide_others, &show_all, &quit],
        )
        .map_err(|e| e.to_string())?;
        let edit = Submenu::with_items(
            app,
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(app, None).map_err(|e| e.to_string())?,
                &PredefinedMenuItem::redo(app, None).map_err(|e| e.to_string())?,
                &PredefinedMenuItem::cut(app, None).map_err(|e| e.to_string())?,
                &PredefinedMenuItem::copy(app, None).map_err(|e| e.to_string())?,
                &PredefinedMenuItem::paste(app, None).map_err(|e| e.to_string())?,
                &PredefinedMenuItem::select_all(app, None).map_err(|e| e.to_string())?,
            ],
        )
        .map_err(|e| e.to_string())?;
        app.set_menu(Menu::with_items(app, &[&submenu, &edit]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        app.on_menu_event(|app, event| {
            if event.id().as_ref() == "ongleet-quit" {
                let _ = app.emit("application-quit-requested", ());
            }
        });
        Ok(())
    }
    fn install_geometry(
        &self,
        w: &tauri::WebviewWindow,
        saved: Arc<Mutex<Geometry>>,
    ) -> Result<(), String> {
        let g = saved.lock().unwrap().clone();
        w.set_size(tauri::LogicalSize::new(
            g.width.clamp(640, 16384),
            g.height.clamp(400, 16384),
        ))
        .map_err(|e| e.to_string())?;
        if let Some((x, y)) = g.position {
            if w.available_monitors()
                .map_err(|e| e.to_string())?
                .iter()
                .any(|m| {
                    let p = m.position().to_logical::<f64>(m.scale_factor());
                    let size = m.size().to_logical::<f64>(m.scale_factor());
                    (x as f64) < p.x + size.width
                        && (x as f64) + g.width as f64 > p.x
                        && (y as f64) >= p.y
                        && (y as f64) + 40.0 < p.y + size.height
                })
            {
                w.set_position(tauri::LogicalPosition::new(x, y))
                    .map_err(|e| e.to_string())?;
            }
        }
        if g.maximized {
            w.maximize().map_err(|e| e.to_string())?;
        }
        let window = w.clone();
        w.on_window_event(move |event| {
            let Ok(scale) = window.scale_factor() else {
                return;
            };
            let mut g = saved.lock().unwrap();
            g.maximized = window.is_maximized().unwrap_or(g.maximized);
            if g.maximized || window.is_minimized().unwrap_or(false) {
                return;
            }
            match event {
                tauri::WindowEvent::Resized(size) => {
                    let size = size.to_logical::<i32>(scale);
                    if size.width >= 640 && size.height >= 400 {
                        g.width = size.width;
                        g.height = size.height;
                    }
                }
                tauri::WindowEvent::Moved(p) => {
                    let p = p.to_logical::<i32>(scale);
                    g.position = Some((p.x, p.y));
                }
                _ => (),
            }
        });
        Ok(())
    }
    fn clipboard(
        &self,
        app: tauri::AppHandle,
        primary: bool,
    ) -> SystemFuture<'_, ClipboardContent> {
        Box::pin(async move {
            if primary {
                return Ok(ClipboardContent::Text(
                    self.primary.lock().map_err(|e| e.to_string())?.clone(),
                ));
            }
            let (tx, rx) = mpsc::sync_channel(1);
            app.run_on_main_thread(move || {
                let result = super::clipboard::read();
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
            tauri::async_runtime::spawn_blocking(move || {
                rx.recv_timeout(Duration::from_secs(3))
                    .map_err(|_| "Clipboard request timed out".to_string())?
            })
            .await
            .map_err(|e| e.to_string())?
        })
    }
    fn write_primary(&self, _app: tauri::AppHandle, text: String) -> SystemFuture<'_, ()> {
        Box::pin(async move {
            *self.primary.lock().map_err(|e| e.to_string())? = text;
            Ok(())
        })
    }
    fn local_file(&self, uri: &str) -> Option<PathBuf> {
        reqwest::Url::parse(uri).ok()?.to_file_path().ok()
    }
    fn file_uri(&self, path: &Path) -> String {
        reqwest::Url::from_file_path(path)
            .map(|u| u.to_string())
            .unwrap_or_default()
    }
    fn open_uri(&self, uri: String) -> SystemFuture<'_, ()> {
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                let status = std::process::Command::new("/usr/bin/open")
                    .arg("--")
                    .arg(uri)
                    .status()
                    .map_err(|e| e.to_string())?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("Cannot open link: {status}"))
                }
            })
            .await
            .map_err(|e| e.to_string())?
        })
    }
    fn page_load(&self, event: String) {
        if let Some(record) = self.diagnostics.lock().unwrap().as_ref() {
            record(NativeDiagnostic::Load(event));
        }
    }
    fn install_diagnostics(
        &self,
        _app: &tauri::App,
        record: Arc<dyn Fn(NativeDiagnostic) + Send + Sync>,
    ) -> Result<(), String> {
        record(NativeDiagnostic::Installed);
        *self.diagnostics.lock().unwrap() = Some(record);
        Ok(())
    }
    fn install_input_diagnostics(
        &self,
        app: &tauri::App,
        record: Arc<dyn Fn(NativeInputDiagnostic) + Send + Sync>,
    ) -> Result<(), String> {
        use super::super::NativeInputEvent;
        let window = app
            .get_webview_window("main")
            .ok_or("Main window unavailable")?;
        let label = window.label().to_string();
        let start = std::time::Instant::now();
        record(NativeInputDiagnostic::Enabled {
            label: label.clone(),
            active: window.is_focused().unwrap_or(false),
            visible: window.is_visible().unwrap_or(false),
        });
        let focus_record = record.clone();
        let focus_window = window.clone();
        let focus_label = label.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                focus_record(NativeInputDiagnostic::Event(NativeInputEvent {
                    elapsed_ms: start.elapsed().as_millis(),
                    label: focus_label.clone(),
                    event: "focus".into(),
                    active: *focused,
                    focused: *focused,
                    visible: focus_window.is_visible().unwrap_or(false),
                    send_event: false,
                    key: None,
                }));
            }
        });
        super::input::install(label, record, move || {
            (
                window.is_focused().unwrap_or(false),
                window.is_visible().unwrap_or(false),
            )
        })?;
        Ok(())
    }
}
