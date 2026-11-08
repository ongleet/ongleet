use super::super::{
    ClipboardContent, DesktopService, Geometry, NativeDiagnostic, NativeInputDiagnostic,
    PlatformCapabilities, SystemFuture,
};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
use tauri::Manager;
impl DesktopService for super::Windows {
    fn capabilities(&self) -> PlatformCapabilities {
        PlatformCapabilities {
            command_shortcuts: false,
            primary_selection: "internal",
        }
    }
    fn initialize(&self) {}
    fn show(&self, w: &tauri::WebviewWindow) -> Result<(), String> {
        w.set_background_color(Some(tauri::window::Color(16, 18, 22, 255)))
            .map_err(|e| e.to_string())?;
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
                let result = read_clipboard();
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
        let url = reqwest::Url::parse(uri).ok()?;
        if url
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
        {
            return None;
        }
        url.to_file_path().ok()
    }
    fn file_uri(&self, path: &Path) -> String {
        reqwest::Url::from_file_path(path)
            .map(|u| u.to_string())
            .unwrap_or_default()
    }
    fn open_uri(&self, uri: String) -> SystemFuture<'_, ()> {
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                let operation = super::native::wide("open").map_err(|e| e.to_string())?;
                let uri = super::native::wide(uri).map_err(|e| e.to_string())?;
                let result = unsafe {
                    windows_sys::Win32::UI::Shell::ShellExecuteW(
                        std::ptr::null_mut(),
                        operation.as_ptr(),
                        uri.as_ptr(),
                        std::ptr::null(),
                        std::ptr::null(),
                        windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
                    )
                };
                if result as isize <= 32 {
                    Err(format!("Cannot open link: {}", result as isize))
                } else {
                    Ok(())
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
        Ok(())
    }
}

fn read_clipboard() -> Result<ClipboardContent, String> {
    use windows_sys::Win32::{
        System::{DataExchange::*, Memory::*},
        UI::Shell::*,
    };
    struct Close;
    impl Drop for Close {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    unsafe {
        super::native::check(OpenClipboard(std::ptr::null_mut())).map_err(|e| e.to_string())?;
        let _close = Close;
        // Files and images take precedence over their textual representations.
        if IsClipboardFormatAvailable(15) != 0 {
            let handle = GetClipboardData(15);
            if handle.is_null() {
                return Err("Clipboard file list unavailable".into());
            }
            let count = DragQueryFileW(handle, u32::MAX, std::ptr::null_mut(), 0);
            if count > 4096 {
                return Err("Clipboard file list exceeds 4096 paths".into());
            }
            let mut files = Vec::new();
            for index in 0..count {
                let length = DragQueryFileW(handle, index, std::ptr::null_mut(), 0);
                if length > 32768 {
                    return Err("Clipboard path is too long".into());
                }
                let mut name = vec![0u16; length as usize + 1];
                DragQueryFileW(handle, index, name.as_mut_ptr(), length + 1);
                files.push(PathBuf::from(String::from_utf16_lossy(
                    &name[..length as usize],
                )));
            }
            return Ok(ClipboardContent::Files(files));
        }
        if IsClipboardFormatAvailable(8) != 0 || IsClipboardFormatAvailable(17) != 0 {
            return Ok(ClipboardContent::Image);
        }
        if IsClipboardFormatAvailable(13) == 0 {
            return Ok(ClipboardContent::Text(String::new()));
        }
        let handle = GetClipboardData(13);
        if handle.is_null() {
            return Err("Clipboard text unavailable".into());
        }
        let size = GlobalSize(handle);
        if size > 16 * 1024 * 1024 {
            return Err("Clipboard text exceeds 16 MiB".into());
        }
        let pointer = GlobalLock(handle) as *const u16;
        if pointer.is_null() {
            return Err("Clipboard text cannot be locked".into());
        }
        let bytes = std::slice::from_raw_parts(pointer, size / 2);
        let length = bytes
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(bytes.len());
        let text = String::from_utf16_lossy(&bytes[..length]);
        GlobalUnlock(handle);
        Ok(ClipboardContent::Text(text))
    }
}
