use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;

use crate::system::Geometry;

pub struct WindowState {
    path: PathBuf,
    geometry: Arc<Mutex<Geometry>>,
}

impl WindowState {
    pub fn install(app: &tauri::App, directory: PathBuf) -> tauri::Result<()> {
        let path = directory.join("window-state.json");
        let geometry: Geometry = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Geometry::default(),
        };
        let window = app.get_webview_window("main").expect("main window");
        let geometry = Arc::new(Mutex::new(geometry));
        app.state::<crate::system::SystemServices>()
            .desktop
            .install_geometry(&window, geometry.clone())
            .map_err(std::io::Error::other)?;
        app.manage(Self { path, geometry });
        app.state::<crate::system::SystemServices>()
            .desktop
            .show(&window)
            .map_err(std::io::Error::other)?;
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
    window
        .state::<crate::system::SystemServices>()
        .desktop
        .show(window)
        .map_err(std::io::Error::other)?;
    let lifecycle = window.state::<crate::Lifecycle>();
    lifecycle.cancel_close();
    lifecycle
        .visible
        .store(true, std::sync::atomic::Ordering::Release);
    window
        .state::<crate::system::SystemServices>()
        .desktop
        .activate(window)
        .map_err(std::io::Error::other)?;
    Ok(())
}
