//! A dedicated user-scoped DPAPI store. No Credential Manager blob-size limit.
use super::{
    super::{
        PrivateOpen, SecretAttributes, SecretRecord, SecretSearch, SecretStore, SystemFiles,
        SystemFuture, SystemPaths,
    },
    native::*,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::LocalFree, Security::Cryptography::*};
const PAYLOAD_LIMIT: usize = 64 * 1024;
const STORE_LIMIT: u64 = 32 * 1024 * 1024;
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct Entry {
    attributes: SecretAttributes,
    bytes: Vec<u8>,
}
type Entries = BTreeMap<String, Entry>;
fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes
            .len()
            .try_into()
            .map_err(|_| "Credential store is too large")?,
        pbData: bytes.as_ptr() as *mut _,
    };
    let entropy_bytes = b"io.local.ongleet.dpapi.credentials.v1";
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_ptr() as *mut _,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: null_mut(),
    };
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                null(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    check(ok).map_err(|e| format!("DPAPI credential protection failed: {e}"))?;
    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData.cast());
    }
    Ok(result)
}
pub(super) fn with_store<T>(
    native: &super::Windows,
    path: &Path,
    write: bool,
    operation: impl FnOnce(&mut Entries) -> Result<T, String>,
) -> Result<T, String> {
    native
        .private_directory(
            path.parent().ok_or("Credential directory unavailable")?,
            true,
        )
        .map_err(|e| e.to_string())?;
    let _lock = native.try_lock(&path.with_extension("lock"))?;
    let mut entries = match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(STORE_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > STORE_LIMIT {
                return Err("Credential store exceeds 32 MiB".into());
            }
            let mut plain = crypt(&bytes, false)?;
            let result = serde_json::from_slice::<Entries>(&plain)
                .map_err(|_| "Invalid decrypted credential store".to_string());
            plain.fill(0);
            result?
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e.to_string()),
    };
    let result = operation(&mut entries)?;
    if write {
        let mut plain = serde_json::to_vec(&entries).map_err(|e| e.to_string())?;
        let ciphertext = crypt(&plain, true);
        plain.fill(0);
        let ciphertext = ciphertext?;
        if ciphertext.len() as u64 > STORE_LIMIT {
            return Err("Credential store exceeds 32 MiB".into());
        }
        let temporary = path.with_extension(format!("{}.tmp", native.nonce()?));
        let saved = (|| {
            let mut file = native
                .open_private(&temporary, PrivateOpen::New)
                .map_err(|e| e.to_string())?;
            file.write_all(&ciphertext)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            native
                .atomic_replace(&temporary, path)
                .map_err(|e| e.to_string())
        })();
        let _ = fs::remove_file(temporary);
        saved?;
    }
    for entry in entries.values_mut() {
        entry.bytes.fill(0);
    }
    Ok(result)
}
fn path(native: &super::Windows) -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(path) = &native.secret_path {
        return Ok(path.clone());
    }
    Ok(native
        .data_directory("ongleet")?
        .join("credentials/dpapi-v1.bin"))
}
impl SecretStore for super::Windows {
    fn ensure_available(&self) -> SystemFuture<'_, ()> {
        Box::pin(async move { with_store(self, &path(self)?, false, |_| Ok(())) })
    }
    fn read<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, Vec<u8>> {
        Box::pin(async move {
            with_store(self, &path(self)?, false, |entries| {
                let entry = entries
                    .get(&record.identity)
                    .filter(|entry| entry.attributes == record.attributes)
                    .ok_or("Credential record unavailable")?;
                if entry.bytes.len() > PAYLOAD_LIMIT {
                    return Err("Credential exceeds 64 KiB".into());
                }
                Ok(entry.bytes.clone())
            })
        })
    }
    fn search<'a>(&'a self, attributes: &'a SecretAttributes) -> SystemFuture<'a, SecretSearch> {
        Box::pin(async move {
            with_store(self, &path(self)?, false, |entries| {
                Ok(SecretSearch {
                    locked: false,
                    records: entries
                        .iter()
                        .filter(|(_, entry)| {
                            attributes
                                .iter()
                                .all(|(key, value)| entry.attributes.get(key) == Some(value))
                        })
                        .map(|(identity, entry)| SecretRecord {
                            identity: identity.clone(),
                            attributes: entry.attributes.clone(),
                        })
                        .collect(),
                })
            })
        })
    }
    fn create<'a>(
        &'a self,
        attributes: &'a SecretAttributes,
        bytes: &'a [u8],
        replace: bool,
    ) -> SystemFuture<'a, ()> {
        Box::pin(async move {
            if bytes.len() > PAYLOAD_LIMIT {
                return Err("Credential exceeds 64 KiB".into());
            }
            with_store(self, &path(self)?, true, |entries| {
                if replace {
                    entries.retain(|_, entry| entry.attributes != *attributes);
                }
                entries.insert(
                    self.nonce()?,
                    Entry {
                        attributes: attributes.clone(),
                        bytes: bytes.to_vec(),
                    },
                );
                Ok(())
            })
        })
    }
    fn remove<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, ()> {
        Box::pin(async move {
            with_store(self, &path(self)?, true, |entries| {
                if entries
                    .get(&record.identity)
                    .is_some_and(|entry| entry.attributes == record.attributes)
                {
                    entries.remove(&record.identity);
                }
                Ok(())
            })
        })
    }
}

#[cfg(test)]
pub(super) fn test_roundtrip(native: &super::Windows, path: &Path, attributes: SecretAttributes) {
    let secret = vec![b's'; PAYLOAD_LIMIT];
    with_store(native, path, true, |entries| {
        entries.insert(
            "record".into(),
            Entry {
                attributes: attributes.clone(),
                bytes: secret.clone(),
            },
        );
        Ok(())
    })
    .unwrap();
    let ciphertext = fs::read(path).unwrap();
    assert!(!ciphertext
        .windows(128)
        .any(|window| window == &secret[..128]));
    with_store(native, path, false, |entries| {
        assert_eq!(entries["record"].bytes, secret);
        Ok(())
    })
    .unwrap();
    with_store(native, path, true, |entries| {
        entries.get_mut("record").unwrap().bytes = b"replaced".to_vec();
        Ok(())
    })
    .unwrap();
    with_store(native, path, false, |entries| {
        assert_eq!(entries["record"].bytes, b"replaced");
        Ok(())
    })
    .unwrap();
    with_store(native, path, true, |entries| {
        entries.remove("record");
        Ok(())
    })
    .unwrap();
    with_store(native, path, false, |entries| {
        assert!(entries.is_empty());
        Ok(())
    })
    .unwrap();
    fs::write(path, b"corrupted ciphertext").unwrap();
    assert!(with_store(native, path, false, |_| Ok(())).is_err());
    assert_eq!(fs::read(path).unwrap(), b"corrupted ciphertext");
}
