//! Bounded image reads through the same regular-file boundary as the editor.
use std::{io::Read, path::Path};

pub fn load_with(files: &dyn crate::system::SystemFiles, path: &str) -> Result<Vec<u8>, String> {
    const LIMIT: u64 = 8 * 1024 * 1024;
    if !Path::new(path).is_absolute() || path.len() > 8192 || path.contains('\0') {
        return Err("Choose an absolute file path (up to 8192 bytes)".into());
    }
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let file = files.open_editor(&path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Only regular files can be previewed".into());
    }
    if metadata.len() > LIMIT {
        return Err("Image preview limit is 8 MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Image preview limit is 8 MiB".into());
    }
    Ok(bytes)
}
