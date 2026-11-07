use super::super::{ClipboardContent, DesktopService, Geometry, SystemFuture};
use super::super::{NativeDiagnostic, NativeInputDiagnostic, NativeInputEvent, NativeKey};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Manager;
use webkit2gtk::WebViewExt;
impl DesktopService for super::Linux {
    fn initialize(&self) {
        gtk::glib::set_prgname(Some("Ongleet"));
        gtk::glib::set_application_name("Ongleet");
    }
    fn show(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        window.show().map_err(|e| e.to_string())
    }
    fn hide(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        window.hide().map_err(|e| e.to_string())
    }
    fn activate(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        activate(window).map_err(|e| e.to_string())
    }
    fn install_geometry(
        &self,
        window: &tauri::WebviewWindow,
        saved: Arc<Mutex<Geometry>>,
    ) -> Result<(), String> {
        geometry(window, saved).map_err(|e| e.to_string())
    }
    fn clipboard(
        &self,
        app: tauri::AppHandle,
        primary: bool,
    ) -> SystemFuture<'_, ClipboardContent> {
        Box::pin(async move {
            let (send, receive) = mpsc::sync_channel(1);
            app.run_on_main_thread(move || {
                let clipboard = gtk::Clipboard::get(if primary {
                    &gdk::SELECTION_PRIMARY
                } else {
                    &gdk::SELECTION_CLIPBOARD
                });
                if primary {
                    clipboard.request_text(move |_, text| {
                        let _ = send.send(Ok(ClipboardContent::Text(
                            text.as_deref().unwrap_or_default().into(),
                        )));
                    });
                    return;
                }
                clipboard.request_uris(move |clipboard, uris| {
                    if !uris.is_empty() {
                        let result = uris
                            .iter()
                            .map(|uri| {
                                if !uri.starts_with("file://") {
                                    return Err("Paste supports at most 32 local files".to_string());
                                }
                                gio::File::for_uri(uri)
                                    .path()
                                    .ok_or_else(|| "Clipboard file is not local".to_string())
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .map(ClipboardContent::Files);
                        let _ = send.send(result);
                        return;
                    }
                    clipboard.request_contents(
                        &gdk::Atom::intern("TARGETS"),
                        move |clipboard, data| {
                            let image = data
                                .targets()
                                .unwrap_or_default()
                                .iter()
                                .any(|target| target.name().starts_with("image/"));
                            if image {
                                let _ = send.send(Ok(ClipboardContent::Image));
                            } else {
                                clipboard.request_text(move |_, text| {
                                    let _ = send.send(Ok(ClipboardContent::Text(
                                        text.as_deref().unwrap_or_default().into(),
                                    )));
                                });
                            }
                        },
                    );
                });
            })
            .map_err(|error| error.to_string())?;
            tauri::async_runtime::spawn_blocking(move || {
                receive
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| "Clipboard request timed out".to_string())?
            })
            .await
            .map_err(|error| error.to_string())?
        })
    }
    fn write_primary(&self, app: tauri::AppHandle, text: String) -> SystemFuture<'_, ()> {
        Box::pin(async move {
            let (send, receive) = mpsc::sync_channel(1);
            app.run_on_main_thread(move || {
                gtk::Clipboard::get(&gdk::SELECTION_PRIMARY).set_text(&text);
                let _ = send.send(());
            })
            .map_err(|error| error.to_string())?;
            tauri::async_runtime::spawn_blocking(move || {
                receive
                    .recv_timeout(Duration::from_secs(3))
                    .map_err(|_| "Primary selection request timed out".to_string())
            })
            .await
            .map_err(|error| error.to_string())?
        })
    }
    fn local_file(&self, uri: &str) -> Option<PathBuf> {
        gio::File::for_uri(uri).path()
    }
    fn file_uri(&self, path: &Path) -> String {
        gio::File::for_path(path).uri().to_string()
    }
    fn open_uri(&self, uri: String) -> SystemFuture<'_, ()> {
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
                    .map_err(|error| format!("Cannot open terminal link: {error}"))
            })
            .await
            .map_err(|error| error.to_string())?
        })
    }
    fn install_diagnostics(
        &self,
        app: &tauri::App,
        record: Arc<dyn Fn(NativeDiagnostic) + Send + Sync>,
    ) -> Result<(), String> {
        diagnostics(app, record).map_err(|e| e.to_string())
    }
    fn install_input_diagnostics(
        &self,
        app: &tauri::App,
        record: Arc<dyn Fn(NativeInputDiagnostic) + Send + Sync>,
    ) -> Result<(), String> {
        input_diagnostics(app, record).map_err(|e| e.to_string())
    }
}
fn geometry(window: &tauri::WebviewWindow, saved: Arc<Mutex<Geometry>>) -> tauri::Result<()> {
    let mut geometry = saved.lock().unwrap().clone();
    let native = window.gtk_window()?;
    // GTK uses logical coordinates; the compositor owns placement on Wayland.
    let display = gtk::prelude::WidgetExt::display(&native);
    geometry.width = geometry.width.clamp(640, 16384);
    geometry.height = geometry.height.clamp(400, 16384);
    let monitors: Vec<_> = (0..display.n_monitors())
        .filter_map(|index| display.monitor(index))
        .map(|monitor| monitor.workarea())
        .collect();
    if let Some(area) = display.primary_monitor().map(|monitor| monitor.workarea()) {
        geometry.width = geometry.width.min(area.width());
        geometry.height = geometry.height.min(area.height());
    }
    native.resize(geometry.width, geometry.height);
    if let Some((x, y)) = geometry.position {
        if monitors.iter().any(|area| {
            i64::from(x) < i64::from(area.x()) + i64::from(area.width())
                && i64::from(x) + i64::from(geometry.width) > i64::from(area.x())
                && y >= area.y()
                && i64::from(y) + 40 < i64::from(area.y()) + i64::from(area.height())
        }) {
            native.move_(x, y);
        }
    }
    if geometry.maximized {
        native.maximize();
    }
    *saved.lock().unwrap() = geometry;
    let geometry = saved;
    let current = geometry.clone();
    native.connect_configure_event(move |window, _| {
        let state = window
            .window()
            .map(|window| window.state())
            .unwrap_or_else(gdk::WindowState::empty);
        if !state.intersects(
            gdk::WindowState::MAXIMIZED
                | gdk::WindowState::ICONIFIED
                | gdk::WindowState::FULLSCREEN,
        ) && !window.is_maximized()
        {
            let (width, height) = window.size();
            if width >= 640 && height >= 400 {
                let mut saved = current.lock().unwrap();
                saved.width = width;
                saved.height = height;
                saved.position = Some(window.position());
            }
        }
        false
    });
    let current = geometry.clone();
    native.connect_window_state_event(move |_, event| {
        current.lock().unwrap().maximized = event
            .new_window_state()
            .contains(gdk::WindowState::MAXIMIZED);
        gtk::glib::Propagation::Proceed
    });

    Ok(())
}
fn activate(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let native = window.gtk_window()?;
    // Wayland has no unminimize request. Presenting an inactive existing surface
    // may also be denied by compositor activation policy. An explicit Show
    // remaps that surface while retaining the GTK window, WebView and PTYs.
    if gtk::prelude::WidgetExt::display(&native).type_().name() == "GdkWaylandDisplay"
        && native.is_visible()
        && !native.is_active()
    {
        native.hide();
    }
    // Tao queues unminimize but drops set_focus while still marked minimized.
    // Present directly rather than racing those two asynchronous requests.
    native.present_with_time(0);

    Ok(())
}
fn diagnostics(
    app: &tauri::App,
    record: Arc<dyn Fn(NativeDiagnostic) + Send + Sync>,
) -> tauri::Result<()> {
    for (_, window) in app.webview_windows() {
        let record = record.clone();
        window.with_webview(move |platform| {
            let webview = platform.inner();
            let load = record.clone();
            let failed = record.clone();
            let terminated = record.clone();
            let resource = record;
            webview.connect_load_changed(move |_, event| {
                load(NativeDiagnostic::Load(format!("{event:?}")))
            });
            webview.connect_load_failed(move |_, event, _, error| {
                failed(NativeDiagnostic::LoadFailed {
                    stage: format!("{event:?}"),
                    domain: format!("{:?}", error.domain()),
                });
                false
            });
            webview.connect_web_process_terminated(move |_, reason| {
                terminated(NativeDiagnostic::Terminated(format!("{reason:?}")))
            });
            webview.connect_resource_load_started(move |_, _, _| {
                resource(NativeDiagnostic::ResourceStarted)
            });
        })?;
    }
    let heartbeat = record.clone();
    let mut ticks = 0u64;
    gtk::glib::timeout_add_seconds_local(30, move || {
        ticks += 1;
        // Only selected process counters, never environment or terminal contents.
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status
                .lines()
                .filter(|line| line.starts_with("VmRSS:") || line.starts_with("Threads:"))
            {
                heartbeat(NativeDiagnostic::Heartbeat {
                    tick: ticks,
                    counter: line.into(),
                });
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    record(NativeDiagnostic::Installed);
    Ok(())
}

fn input_diagnostics(
    app: &tauri::App,
    record: Arc<dyn Fn(NativeInputDiagnostic) + Send + Sync>,
) -> tauri::Result<()> {
    let started = Instant::now();
    for (label, window) in app.webview_windows() {
        let native = window.gtk_window()?;
        record(NativeInputDiagnostic::Enabled {
            label: label.clone(),
            active: native.is_active(),
            visible: native.is_visible(),
        });
        let record = record.clone();
        native.connect_event(move |window, event| {
            let key = event.downcast_ref::<gdk::EventKey>().map(|key| NativeKey {
                time: key.time(),
                hardware_code: key.hardware_keycode(),
                keyval: format!("{:?}", key.keyval()),
                group: key.group(),
                modifiers: format!("{:?}", key.state()),
            });
            if key.is_some() || matches!(event.event_type(), gdk::EventType::FocusChange) {
                record(NativeInputDiagnostic::Event(NativeInputEvent {
                    elapsed_ms: started.elapsed().as_millis(),
                    label: label.clone(),
                    event: if key.is_some() {
                        format!("{:?}", event.event_type())
                    } else {
                        "focus".into()
                    },
                    active: window.is_active(),
                    focused: window.has_toplevel_focus(),
                    visible: window.is_visible(),
                    send_event: event.is_send_event(),
                    key,
                }));
            }
            glib::Propagation::Proceed
        });
    }
    Ok(())
}
