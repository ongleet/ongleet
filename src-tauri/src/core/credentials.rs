//! Plugin credential identities and migration policy; no plaintext fallback.
use crate::system::{SecretAttributes, SecretStore};
pub enum Operation {
    Get,
    Set(String),
    Delete,
}
pub async fn migrate_with(store: &dyn SecretStore) -> Result<(), String> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;
    tokio::time::timeout(std::time::Duration::from_secs(3), migrate_inner(store))
        .await
        .map_err(|_| {
            "Credential migration timed out; retry when storage is available".to_string()
        })?
}
async fn migrate_inner(store: &dyn SecretStore) -> Result<(), String> {
    let unavailable =
        |_| "Credential migration unavailable or locked; legacy entries retained".to_string();
    let old = store
        .search(&SecretAttributes::from([(
            "application".into(),
            "devcenter".into(),
        )]))
        .await
        .map_err(unavailable)?;
    if old.locked {
        return Err(unavailable(String::new()));
    }
    if old.records.is_empty() {
        return Ok(());
    }
    store.ensure_available().await.map_err(unavailable)?;
    let mut pending = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in old.records {
        let mut attributes = item.attributes.clone();
        attributes.insert("application".into(), "ongleet".into());
        if !seen.insert(attributes.clone()) {
            return Err("Conflicting duplicate legacy credentials; legacy entries retained".into());
        }
        let target = store.search(&attributes).await.map_err(unavailable)?;
        if target.locked {
            return Err(unavailable(String::new()));
        }
        let secret = store.read(&item).await.map_err(unavailable)?;
        for existing in &target.records {
            if store.read(existing).await.map_err(unavailable)? != secret {
                return Err("Conflicting Ongleet credential; legacy entries retained".into());
            }
        }
        pending.push((item, attributes, secret, target.records.is_empty()));
    }
    for (item, attributes, secret, missing) in pending {
        if missing {
            store
                .create(&attributes, &secret, false)
                .await
                .map_err(unavailable)?;
        }
        store.remove(&item).await.map_err(unavailable)?;
    }
    Ok(())
}
pub async fn execute_with(
    store: &dyn SecretStore,
    plugin: &str,
    key: &str,
    operation: Operation,
) -> Result<serde_json::Value, String> {
    if key.is_empty() || key.len() > 128 || key.chars().any(char::is_control) {
        return Err("Invalid credential key".into());
    }
    if matches!(&operation, Operation::Set(value) if value.len() > 65_536) {
        return Err("Credential exceeds 64 KiB".into());
    }
    migrate_with(store).await?;
    store.ensure_available().await?;
    let attributes = SecretAttributes::from([
        ("application".into(), "ongleet".into()),
        ("plugin".into(), plugin.into()),
        ("key".into(), key.into()),
    ]);
    let found = store.search(&attributes).await?;
    if found.locked {
        return Err("Credential storage unavailable or locked".into());
    }
    match operation {
        Operation::Get => match found.records.first() {
            Some(record) => {
                let bytes = store.read(record).await?;
                if bytes.len() > 65_536 {
                    return Err("Credential exceeds 64 KiB".into());
                }
                Ok(serde_json::Value::String(
                    String::from_utf8(bytes).map_err(|_| "Invalid credential encoding")?,
                ))
            }
            None => Ok(serde_json::Value::Null),
        },
        Operation::Delete => {
            for record in found.records {
                store.remove(&record).await?;
            }
            Ok(serde_json::Value::Null)
        }
        Operation::Set(value) => {
            store.create(&attributes, value.as_bytes(), true).await?;
            Ok(serde_json::Value::Null)
        }
    }
}
#[cfg(test)]
async fn migrate() -> Result<(), String> {
    migrate_with(crate::system::services().secrets.as_ref()).await
}
#[cfg(test)]
async fn execute(
    plugin: &str,
    key: &str,
    operation: Operation,
) -> Result<serde_json::Value, String> {
    execute_with(
        crate::system::services().secrets.as_ref(),
        plugin,
        key,
        operation,
    )
    .await
}
#[cfg(test)]
#[cfg(target_os = "linux")]
use secret_service::{EncryptionType, SecretService};
#[cfg(test)]
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires an isolated dbus-run-session with an unlocked Secret Service"]
    fn isolated_secret_service_persists_and_isolates_plugin_records() {
        assert!(std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .unwrap()
            .contains("/tmp/"));
        tauri::async_runtime::block_on(async {
            execute("fixture-a", "account", Operation::Set("first".into()))
                .await
                .unwrap();
            // Each operation opens a new service connection, as after an app restart.
            assert_eq!(
                execute("fixture-a", "account", Operation::Get)
                    .await
                    .unwrap(),
                "first"
            );
            assert_eq!(
                execute("fixture-b", "account", Operation::Get)
                    .await
                    .unwrap(),
                serde_json::Value::Null
            );
            execute("fixture-a", "account", Operation::Set("replacement".into()))
                .await
                .unwrap();
            assert_eq!(
                execute("fixture-a", "account", Operation::Get)
                    .await
                    .unwrap(),
                "replacement"
            );
            execute("fixture-a", "account", Operation::Delete)
                .await
                .unwrap();
            assert_eq!(
                execute("fixture-a", "account", Operation::Get)
                    .await
                    .unwrap(),
                serde_json::Value::Null
            );
            let service = SecretService::connect(EncryptionType::Dh).await.unwrap();
            service
                .get_default_collection()
                .await
                .unwrap()
                .lock()
                .await
                .unwrap();
            assert!(execute("fixture-a", "account", Operation::Get)
                .await
                .unwrap_err()
                .contains("locked"));
        });
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod migration_tests {
    use super::*;
    #[test]
    #[ignore = "requires isolated dbus-run-session and unlocked temporary keyring"]
    fn copies_legacy_records_detects_conflict_and_keeps_locked_entries() {
        assert!(std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .unwrap()
            .contains("/tmp/"));
        assert!(std::env::var("XDG_DATA_HOME").unwrap().starts_with("/tmp/"));
        tauri::async_runtime::block_on(async {
            let service = SecretService::connect(EncryptionType::Dh).await.unwrap();
            let collection = service.get_default_collection().await.unwrap();
            let attrs = HashMap::from([
                ("application", "devcenter"),
                ("plugin", "fixture-migration"),
                ("key", "account"),
            ]);
            collection
                .create_item(
                    "Legacy fixture",
                    attrs.clone(),
                    b"preserved",
                    true,
                    "text/plain",
                )
                .await
                .unwrap();
            migrate().await.unwrap();
            migrate().await.unwrap();
            assert!(service
                .search_items(attrs.clone())
                .await
                .unwrap()
                .unlocked
                .is_empty());
            assert_eq!(
                execute("fixture-migration", "account", Operation::Get)
                    .await
                    .unwrap(),
                "preserved"
            );
            collection
                .create_item(
                    "Conflicting fixture",
                    attrs.clone(),
                    b"different",
                    true,
                    "text/plain",
                )
                .await
                .unwrap();
            assert!(migrate().await.unwrap_err().contains("Conflicting"));
            assert_eq!(
                service.search_items(attrs.clone()).await.unwrap().unlocked[0]
                    .get_secret()
                    .await
                    .unwrap(),
                b"different"
            );
            for item in service
                .search_items(HashMap::from([
                    ("application", "ongleet"),
                    ("plugin", "fixture-migration"),
                    ("key", "account"),
                ]))
                .await
                .unwrap()
                .unlocked
            {
                item.delete().await.unwrap();
            }
            collection.lock().await.unwrap();
            assert!(migrate().await.unwrap_err().contains("locked"));
            // Unlock is deliberately left to the caller's isolated keyring.
            // Retrying before locking already exercised completed migration.
        });
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    use crate::system::{SecretRecord, SecretSearch, SystemFuture};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };

    #[derive(Default)]
    struct MemoryStore {
        records: Mutex<Vec<(SecretRecord, Vec<u8>)>>,
        fail_reads: AtomicBool,
        locked: AtomicBool,
    }
    impl MemoryStore {
        fn insert(&self, application: &str, key: &str, bytes: &[u8]) {
            let mut records = self.records.lock().unwrap();
            let identity = records.len().to_string();
            records.push((
                SecretRecord {
                    identity,
                    attributes: SecretAttributes::from([
                        ("application".into(), application.into()),
                        ("plugin".into(), "fixture".into()),
                        ("key".into(), key.into()),
                    ]),
                },
                bytes.to_vec(),
            ));
        }
    }
    impl SecretStore for MemoryStore {
        fn search<'a>(
            &'a self,
            attributes: &'a SecretAttributes,
        ) -> SystemFuture<'a, SecretSearch> {
            Box::pin(async move {
                Ok(SecretSearch {
                    locked: self.locked.load(Ordering::Acquire),
                    records: self
                        .records
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|(record, _)| {
                            attributes
                                .iter()
                                .all(|(key, value)| record.attributes.get(key) == Some(value))
                        })
                        .map(|(record, _)| SecretRecord {
                            identity: record.identity.clone(),
                            attributes: record.attributes.clone(),
                        })
                        .collect(),
                })
            })
        }
        fn ensure_available(&self) -> SystemFuture<'_, ()> {
            Box::pin(async {
                if self.locked.load(Ordering::Acquire) {
                    Err("Credential storage unavailable or locked".into())
                } else {
                    Ok(())
                }
            })
        }
        fn read<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, Vec<u8>> {
            Box::pin(async move {
                if self.fail_reads.load(Ordering::Acquire) {
                    return Err("Credential storage unavailable or locked".into());
                }
                self.records
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|(found, _)| found.identity == record.identity)
                    .map(|(_, bytes)| bytes.clone())
                    .ok_or_else(|| "Missing fixture".into())
            })
        }
        fn create<'a>(
            &'a self,
            attributes: &'a SecretAttributes,
            bytes: &'a [u8],
            replace: bool,
        ) -> SystemFuture<'a, ()> {
            Box::pin(async move {
                let mut records = self.records.lock().unwrap();
                if replace {
                    records.retain(|(record, _)| &record.attributes != attributes);
                }
                let identity = format!("created-{}", records.len());
                records.push((
                    SecretRecord {
                        identity,
                        attributes: attributes.clone(),
                    },
                    bytes.to_vec(),
                ));
                Ok(())
            })
        }
        fn remove<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, ()> {
            Box::pin(async move {
                self.records
                    .lock()
                    .unwrap()
                    .retain(|(found, _)| found.identity != record.identity);
                Ok(())
            })
        }
    }
    #[test]
    fn migration_preflights_every_record_before_copying_or_deleting() {
        tauri::async_runtime::block_on(async {
            let store = MemoryStore::default();
            store.insert("devcenter", "first", b"first legacy");
            store.insert("devcenter", "second", b"second legacy");
            store.insert("ongleet", "second", b"conflicting destination");
            assert!(migrate_with(&store)
                .await
                .unwrap_err()
                .contains("Conflicting"));
            let records = store.records.lock().unwrap();
            assert_eq!(records.len(), 3);
            assert_eq!(records[0].1, b"first legacy");
            assert_eq!(records[1].1, b"second legacy");
            assert_eq!(records[2].1, b"conflicting destination");
        });
    }
    #[test]
    fn locked_migration_retains_legacy_and_delete_does_not_read_secret_bytes() {
        tauri::async_runtime::block_on(async {
            let store = MemoryStore::default();
            store.insert("devcenter", "account", b"legacy");
            store.locked.store(true, Ordering::Release);
            assert!(migrate_with(&store).await.unwrap_err().contains("locked"));
            assert_eq!(store.records.lock().unwrap().len(), 1);
            store.locked.store(false, Ordering::Release);
            migrate_with(&store).await.unwrap();
            let records = store.records.lock().unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].0.attributes["application"], "ongleet");
            assert_eq!(records[0].1, b"legacy");
            drop(records);
            store.fail_reads.store(true, Ordering::Release);
            assert!(execute_with(&store, "fixture", "account", Operation::Get)
                .await
                .unwrap_err()
                .contains("locked"));
            execute_with(&store, "fixture", "account", Operation::Delete)
                .await
                .unwrap();
            assert!(store.records.lock().unwrap().is_empty());
        });
    }
}
