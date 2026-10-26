use gio::prelude::*;
use gtk::{gdk, gio};
use serde::Serialize;
use std::{sync::mpsc, time::Duration};

#[derive(Serialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum Paste {
    Text(String),
    Image,
}

// Request clipboard conversion asynchronously on GTK's thread. Never wait on
// the WebView thread or hold the terminal registry while another app responds.
#[tauri::command]
pub async fn terminal_clipboard(
    app: tauri::AppHandle,
    primary: Option<bool>,
) -> Result<Paste, String> {
    let (send, receive) = mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let clipboard = gtk::Clipboard::get(if primary.unwrap_or(false) {
            &gdk::SELECTION_PRIMARY
        } else {
            &gdk::SELECTION_CLIPBOARD
        });
        if primary.unwrap_or(false) {
            clipboard.request_text(move |_, text| {
                let _ = send.send(clipboard_text(text.as_deref().unwrap_or_default()));
            });
            return;
        }
        clipboard.request_uris(move |clipboard, uris| {
            if !uris.is_empty() {
                let result = file_paths(uris.iter().map(|uri| uri.as_str())).map(Paste::Text);
                let _ = send.send(result);
                return;
            }
            clipboard.request_contents(&gdk::Atom::intern("TARGETS"), move |clipboard, data| {
                let image = data
                    .targets()
                    .unwrap_or_default()
                    .iter()
                    .any(|target| target.name().starts_with("image/"));
                if image {
                    let _ = send.send(Ok(Paste::Image));
                } else {
                    clipboard.request_text(move |_, text| {
                        let _ = send.send(clipboard_text(text.as_deref().unwrap_or_default()));
                    });
                }
            });
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
}

fn clipboard_text(text: &str) -> Result<Paste, String> {
    if text.len() > 65500 {
        Err("Clipboard exceeds the terminal input limit".into())
    } else {
        Ok(Paste::Text(text.into()))
    }
}

#[tauri::command]
pub async fn write_primary_selection(app: tauri::AppHandle, text: String) -> Result<(), String> {
    if text.len() > 65500 {
        return Err("Primary selection exceeds the terminal input limit".into());
    }
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
}

fn file_paths<'a>(uris: impl Iterator<Item = &'a str>) -> Result<String, String> {
    let mut paths = Vec::new();
    let mut length = 0;
    for uri in uris {
        if paths.len() >= 32 || !uri.starts_with("file://") {
            return Err("Paste supports at most 32 local files".into());
        }
        let path = gio::File::for_uri(uri)
            .path()
            .ok_or("Clipboard file is not local")?;
        let path = path.to_str().ok_or("Clipboard path is not valid UTF-8")?;
        if path.chars().any(char::is_control) {
            return Err("Clipboard path contains control characters".into());
        }
        let quoted = format!("'{}'", path.replace('\'', "'\\''"));
        length += quoted.len() + 1;
        if length > 65500 {
            return Err("Clipboard paths exceed the terminal input limit".into());
        }
        paths.push(quoted);
    }
    Ok(paths.join(" "))
}

#[tauri::command]
pub async fn open_terminal_link(uri: String) -> Result<(), String> {
    if uri.len() > 8192 || uri.chars().any(char::is_control) {
        return Err("Invalid terminal link".into());
    }
    if !(uri.starts_with("https://") || uri.starts_with("http://") || uri.starts_with("file://")) {
        return Err("Only HTTP(S) and local file links are supported".into());
    }
    if uri.starts_with("file://") {
        let file = gio::File::for_uri(&uri);
        let path = file.path().ok_or("File link is not local")?;
        if !path.exists() {
            return Err("Linked file does not exist".into());
        }
    }
    tauri::async_runtime::spawn_blocking(move || {
        gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>)
            .map_err(|error| format!("Cannot open terminal link: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_files_decode_and_quote_paths_without_shell_expansion() {
        assert_eq!(
            file_paths(["file:///tmp/a%20b.png", "file:///tmp/it%27s%20%24HOME.png"].into_iter())
                .unwrap(),
            "'/tmp/a b.png' '/tmp/it'\\''s $HOME.png'"
        );
        assert!(file_paths(["https://example.com/image.png"].into_iter()).is_err());
        assert!(file_paths(["file:///tmp/bad%0Apath"].into_iter()).is_err());
        assert!(file_paths(std::iter::repeat_n("file:///tmp/image.png", 33)).is_err());
    }
}
