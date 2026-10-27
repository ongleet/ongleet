use gtk::{gdk, glib, prelude::*};
use std::time::Instant;
use tauri::Manager;

/// Opt-in native diagnostics; key values can reveal typed input.
pub fn install(app: &tauri::App) -> tauri::Result<()> {
    if std::env::var_os("ONGLEET_DEBUG").as_deref() != Some(std::ffi::OsStr::new("1")) {
        return Ok(());
    }
    let started = Instant::now();
    for (label, window) in app.webview_windows() {
        let native = window.gtk_window()?;
        eprintln!(
            "[native-input] enabled window={label:?} active={} visible={}",
            native.is_active(),
            native.is_visible()
        );
        // GTK's generic event signal precedes the specific key signals used by Tao.
        native.connect_event(move |window, event| {
            if let Some(key) = event.downcast_ref::<gdk::EventKey>() {
                eprintln!(
                    "[native-input] elapsed_ms={} window={label:?} event={:?} active={} focused={} visible={} gdk_time={} hardware_code={} keyval={:?} group={} modifiers={:?} send_event={}",
                    started.elapsed().as_millis(), event.event_type(),
                    window.is_active(), window.has_toplevel_focus(), window.is_visible(),
                    key.time(), key.hardware_keycode(), key.keyval(), key.group(),
                    key.state(), event.is_send_event(),
                );
            } else if matches!(event.event_type(), gdk::EventType::FocusChange) {
                eprintln!(
                    "[native-input] elapsed_ms={} window={label:?} event=focus active={} focused={} visible={} send_event={}",
                    started.elapsed().as_millis(), window.is_active(),
                    window.has_toplevel_focus(), window.is_visible(), event.is_send_event(),
                );
            }
            glib::Propagation::Proceed
        });
    }
    Ok(())
}
