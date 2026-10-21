use super::*;

#[test]
fn dropping_store_releases_lock_while_a_fork_duplicate_is_still_open() {
    let directory =
        std::env::temp_dir().join(format!("metadata-lock-release-{}", std::process::id()));
    let store = MetadataStore::open(&directory).unwrap();
    // A concurrent PTY fork temporarily inherits this same open description
    // before CLOEXEC takes effect. It must not extend the store's ownership.
    let inherited = store._lock.0.try_clone().unwrap();
    assert!(MetadataStore::open(&directory).is_err());
    drop(store);
    let replacement = MetadataStore::open(&directory).unwrap();
    drop(inherited);
    assert!(MetadataStore::open(&directory).is_err());
    drop(replacement);
    std::fs::remove_dir_all(directory).unwrap();
}
