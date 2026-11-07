use serde::Serialize;
use tauri::Manager;

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
    use crate::system::ClipboardContent;
    match app
        .state::<crate::system::SystemServices>()
        .desktop
        .clipboard(app.clone(), primary.unwrap_or(false))
        .await?
    {
        ClipboardContent::Text(text) => clipboard_text(&text),
        ClipboardContent::Image => Ok(Paste::Image),
        ClipboardContent::Files(paths) => quote_paths(paths.into_iter()).map(Paste::Text),
    }
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
    app.state::<crate::system::SystemServices>()
        .desktop
        .write_primary(app.clone(), text)
        .await
}

fn quote_paths(paths: impl Iterator<Item = std::path::PathBuf>) -> Result<String, String> {
    let mut quoted_paths = Vec::new();
    let mut length = 0;
    for path in paths {
        if quoted_paths.len() >= 32 {
            return Err("Paste supports at most 32 local files".into());
        }
        let path = path.to_str().ok_or("Clipboard path is not valid UTF-8")?;
        if path.chars().any(char::is_control) {
            return Err("Clipboard path contains control characters".into());
        }
        let quoted = format!("'{}'", path.replace('\'', "'\\''"));
        length += quoted.len() + 1;
        if length > 65500 {
            return Err("Clipboard paths exceed the terminal input limit".into());
        }
        quoted_paths.push(quoted);
    }
    Ok(quoted_paths.join(" "))
}

#[tauri::command]
pub async fn open_terminal_link(app: tauri::AppHandle, uri: String) -> Result<(), String> {
    if uri.len() > 8192 || uri.chars().any(char::is_control) {
        return Err("Invalid terminal link".into());
    }
    if !(uri.starts_with("https://") || uri.starts_with("http://") || uri.starts_with("file://")) {
        return Err("Only HTTP(S) and local file links are supported".into());
    }
    if uri.starts_with("file://") {
        let path = app
            .state::<crate::system::SystemServices>()
            .desktop
            .local_file(&uri)
            .ok_or("File link is not local")?;
        if !path.exists() {
            return Err("Linked file does not exist".into());
        }
    }
    app.state::<crate::system::SystemServices>()
        .desktop
        .open_uri(uri)
        .await
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

#[cfg(test)]
fn file_paths<'a>(uris: impl Iterator<Item = &'a str>) -> Result<String, String> {
    let system = crate::system::services();
    let paths = uris
        .map(|uri| {
            if !uri.starts_with("file://") {
                return Err("Paste supports at most 32 local files".to_string());
            }
            system
                .desktop
                .local_file(uri)
                .ok_or_else(|| "Clipboard file is not local".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    quote_paths(paths.into_iter())
}
