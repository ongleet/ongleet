use super::*;
#[test]
fn windows_editor_replacement_preserves_text_format_and_rejects_stale_revisions() {
    let system = crate::system::services();
    let directory = std::env::temp_dir().join(format!(
        "ongleet-win-editor-{}",
        system.files.nonce().unwrap()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("document.txt");
    let name = path.to_str().unwrap();
    std::fs::write(&path, b"\xef\xbb\xbffirst\r\nline\r\n").unwrap();
    let first = load_with(system.files.as_ref(), name).unwrap().unwrap();
    assert!(first.bom);
    assert_eq!(first.line_ending, "CRLF");
    std::fs::write(&path, b"external\r\n").unwrap();
    assert!(save_with(system.files.as_ref(), name, "my edits\n", &first.revision).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"external\r\n");
    let current = load_with(system.files.as_ref(), name).unwrap().unwrap();
    let saved = save_with(system.files.as_ref(), name, "日本語\n", &current.revision).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), "日本語\r\n".as_bytes());
    assert_eq!(
        load_with(system.files.as_ref(), name)
            .unwrap()
            .unwrap()
            .revision,
        saved.revision
    );
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    assert!(save_with(system.files.as_ref(), name, "forbidden", &saved.revision).is_err());
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
