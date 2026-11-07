use super::super::{SecretAttributes, SecretRecord, SecretSearch, SecretStore, SystemFuture};
use secret_service::{EncryptionType, SecretService};
fn unavailable(_: impl std::fmt::Debug) -> String {
    "Credential storage unavailable or locked".into()
}
impl SecretStore for super::Linux {
    fn search<'a>(&'a self, attributes: &'a SecretAttributes) -> SystemFuture<'a, SecretSearch> {
        Box::pin(async move {
            let service = SecretService::connect(EncryptionType::Dh)
                .await
                .map_err(unavailable)?;
            let found = service
                .search_items(
                    attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect(),
                )
                .await
                .map_err(unavailable)?;
            let locked = !found.locked.is_empty();
            let mut records = Vec::new();
            if !locked {
                for item in found.unlocked {
                    records.push(SecretRecord {
                        identity: item.item_path.to_string(),
                        attributes: item
                            .get_attributes()
                            .await
                            .map_err(unavailable)?
                            .into_iter()
                            .collect(),
                    });
                }
            }
            Ok(SecretSearch { locked, records })
        })
    }
    fn ensure_available(&self) -> SystemFuture<'_, ()> {
        Box::pin(async {
            let service = SecretService::connect(EncryptionType::Dh)
                .await
                .map_err(unavailable)?;
            let collection = service
                .get_default_collection()
                .await
                .map_err(unavailable)?;
            if collection.is_locked().await.map_err(unavailable)? {
                return Err(unavailable(()));
            }
            Ok(())
        })
    }
    fn create<'a>(
        &'a self,
        attributes: &'a SecretAttributes,
        bytes: &'a [u8],
        replace: bool,
    ) -> SystemFuture<'a, ()> {
        Box::pin(async move {
            let service = SecretService::connect(EncryptionType::Dh)
                .await
                .map_err(unavailable)?;
            let collection = service
                .get_default_collection()
                .await
                .map_err(unavailable)?;
            if collection.is_locked().await.map_err(unavailable)? {
                return Err(unavailable(()));
            }
            collection
                .create_item(
                    "Ongleet plugin credential",
                    attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect(),
                    bytes,
                    replace,
                    "text/plain",
                )
                .await
                .map_err(unavailable)?;
            Ok(())
        })
    }
    fn read<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let service = SecretService::connect(EncryptionType::Dh)
                .await
                .map_err(unavailable)?;
            let found = service
                .search_items(
                    record
                        .attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect(),
                )
                .await
                .map_err(unavailable)?;
            if !found.locked.is_empty() {
                return Err(unavailable(()));
            }
            for item in found.unlocked {
                if item.item_path.as_str() == record.identity {
                    return item.get_secret().await.map_err(unavailable);
                }
            }
            Err(unavailable(()))
        })
    }
    fn remove<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, ()> {
        Box::pin(async move {
            let service = SecretService::connect(EncryptionType::Dh)
                .await
                .map_err(unavailable)?;
            let found = service
                .search_items(
                    record
                        .attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect(),
                )
                .await
                .map_err(unavailable)?;
            if !found.locked.is_empty() {
                return Err(unavailable(()));
            }
            for item in found.unlocked {
                if item.item_path.as_str() == record.identity {
                    item.delete().await.map_err(unavailable)?;
                }
            }
            Ok(())
        })
    }
}
