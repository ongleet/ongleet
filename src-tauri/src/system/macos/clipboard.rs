//! NSPasteboard access, always called on the main thread.
use super::super::ClipboardContent;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;
pub fn read() -> Result<ClipboardContent, String> {
    let board = NSPasteboard::generalPasteboard();
    let file_type = NSString::from_str("public.file-url");
    let mut files = Vec::new();
    if let Some(items) = board.pasteboardItems() {
        for item in items.iter() {
            if let Some(uri) = item.stringForType(&file_type) {
                if files.len() >= 32 {
                    return Err("Paste supports at most 32 local files".into());
                }
                files.push(
                    reqwest::Url::parse(&uri.to_string())
                        .map_err(|e| e.to_string())?
                        .to_file_path()
                        .map_err(|_| "Clipboard file is not local")?,
                );
            }
        }
    }
    if !files.is_empty() {
        return Ok(ClipboardContent::Files(files));
    }
    if board.types().is_some_and(|types| {
        types.iter().any(|kind| {
            matches!(
                kind.to_string().as_str(),
                "public.png" | "public.tiff" | "public.jpeg" | "public.heic"
            )
        })
    }) {
        return Ok(ClipboardContent::Image);
    }
    Ok(ClipboardContent::Text(
        board
            .stringForType(unsafe { NSPasteboardTypeString })
            .map(|s| s.to_string())
            .unwrap_or_default(),
    ))
}
