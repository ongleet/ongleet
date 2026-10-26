//! Plugin-scoped Linux Secret Service records. No disk or plaintext fallback.
use secret_service::{EncryptionType, SecretService};
use std::collections::HashMap;
pub enum Operation {
    Get,
    Set(String),
    Delete,
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
    let attributes = HashMap::from([
        ("application", "devcenter"),
        ("plugin", plugin),
        ("key", key),
    ]);
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
                    "DevCenter plugin credential",
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
