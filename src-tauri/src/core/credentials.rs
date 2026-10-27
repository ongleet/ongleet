//! Plugin-scoped Linux Secret Service records. No disk or plaintext fallback.
use secret_service::{EncryptionType, SecretService};
use std::collections::HashMap;
pub enum Operation {
    Get,
    Set(String),
    Delete,
}
/// Copy before deleting, comparing all destinations before changing any record.
/// A locked service leaves legacy records available for the next attempt.
pub async fn migrate() -> Result<(), String> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = LOCK.lock().await;
    tokio::time::timeout(std::time::Duration::from_secs(3), migrate_inner())
        .await
        .map_err(|_| {
            "Credential migration timed out; retry when storage is available".to_string()
        })?
}

async fn migrate_inner() -> Result<(), String> {
    let unavailable =
        |_| "Credential migration unavailable or locked; legacy entries retained".to_string();
    let service = SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(unavailable)?;
    let old = service
        .search_items(HashMap::from([("application", "devcenter")]))
        .await
        .map_err(unavailable)?;
    if !old.locked.is_empty() {
        return Err("Credential migration unavailable or locked; legacy entries retained".into());
    }
    if old.unlocked.is_empty() {
        return Ok(());
    }
    let collection = service
        .get_default_collection()
        .await
        .map_err(unavailable)?;
    if collection.is_locked().await.map_err(unavailable)? {
        return Err("Credential migration unavailable or locked; legacy entries retained".into());
    }
    let mut pending = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for item in old.unlocked {
        let mut attributes = item.get_attributes().await.map_err(unavailable)?;
        attributes.insert("application".into(), "ongleet".into());
        let identity: std::collections::BTreeMap<_, _> = attributes.clone().into_iter().collect();
        if !seen.insert(identity) {
            return Err("Conflicting duplicate legacy credentials; legacy entries retained".into());
        }
        let search = attributes
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let target = service.search_items(search).await.map_err(unavailable)?;
        if !target.locked.is_empty() {
            return Err(
                "Credential migration unavailable or locked; legacy entries retained".into(),
            );
        }
        let secret = item.get_secret().await.map_err(unavailable)?;
        for existing in &target.unlocked {
            if existing.get_secret().await.map_err(unavailable)? != secret {
                return Err("Conflicting Ongleet credential; legacy entries retained".into());
            }
        }
        pending.push((item, attributes, secret, target.unlocked.is_empty()));
    }
    for (item, attributes, secret, missing) in pending {
        if missing {
            collection
                .create_item(
                    "Ongleet plugin credential",
                    attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect(),
                    &secret,
                    false,
                    "text/plain",
                )
                .await
                .map_err(unavailable)?;
        }
        item.delete().await.map_err(unavailable)?;
    }
    Ok(())
}

pub async fn execute(
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
    migrate().await?;
    // Error details from the service are deliberately excluded from diagnostics.
    let unavailable = |_| "Credential storage unavailable or locked".to_string();
    let service = SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(unavailable)?;
    let collection = service
        .get_default_collection()
        .await
        .map_err(unavailable)?;
    if collection.is_locked().await.map_err(unavailable)? {
        return Err("Credential storage unavailable or locked".into());
    }
    let attributes = HashMap::from([("application", "ongleet"), ("plugin", plugin), ("key", key)]);
    let found = service
        .search_items(attributes.clone())
        .await
        .map_err(unavailable)?;
    if !found.locked.is_empty() {
        return Err("Credential storage unavailable or locked".into());
    }
    match operation {
        Operation::Get => {
            if let Some(item) = found.unlocked.first() {
                let bytes = item.get_secret().await.map_err(unavailable)?;
                if bytes.len() > 65_536 {
                    return Err("Credential exceeds 64 KiB".into());
                }
                Ok(serde_json::Value::String(
                    String::from_utf8(bytes).map_err(|_| "Invalid credential encoding")?,
                ))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        Operation::Delete => {
            for item in found.unlocked {
                item.delete().await.map_err(unavailable)?;
            }
            Ok(serde_json::Value::Null)
        }
        Operation::Set(value) => {
            collection
                .create_item(
                    "Ongleet plugin credential",
                    attributes,
                    value.as_bytes(),
                    true,
                    "text/plain",
                )
                .await
                .map_err(unavailable)?;
            Ok(serde_json::Value::Null)
        }
    }
}

#[cfg(test)]
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
