use gtk::{gdk, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Geometry {
    width: i32,
    height: i32,
    position: Option<(i32, i32)>,
    maximized: bool,
}

impl Default for Geometry {
    fn default() -> Self {
        Self {
            width: 1100,
            height: 760,
            position: None,
            maximized: false,
        }
    }
}

pub struct WindowState {
    path: PathBuf,
    geometry: Arc<Mutex<Geometry>>,
}

impl WindowState {
    pub fn install(app: &tauri::App, directory: PathBuf) -> tauri::Result<()> {
        let path = directory.join("window-state.json");
        let mut geometry: Geometry = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Geometry::default(),
        };
        let window = app.get_webview_window("main").expect("main window");
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
        let geometry = Arc::new(Mutex::new(geometry));
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
        app.manage(Self { path, geometry });
        window.show()?;
        Ok(())
    }

    pub fn save(&self) {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let bytes = serde_json::to_vec(&*self.geometry.lock().unwrap())?;
            let temporary = self.path.with_extension("json.tmp");
            std::fs::write(&temporary, bytes)?;
            std::fs::rename(temporary, &self.path)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Could not save window state: {error}");
        }
    }
}

/// Activate on the GTK thread; callers need no display-backend knowledge.
pub fn activate(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    window.show()?;
    let lifecycle = window.state::<crate::Lifecycle>();
    lifecycle.cancel_close();
    lifecycle
        .visible
        .store(true, std::sync::atomic::Ordering::Release);
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
