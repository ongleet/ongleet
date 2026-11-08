//! Security.framework generic passwords. No plaintext fallback or shadow index.
use super::super::{SecretAttributes, SecretRecord, SecretSearch, SecretStore, SystemFuture};
use core_foundation::{
    array::CFArray,
    base::{CFType, CFTypeRef, TCFType},
    data::CFData,
    dictionary::{CFDictionary, CFDictionaryRef},
    string::{CFString, CFStringRef},
};
use std::ffi::c_void;
#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecClass: CFStringRef;
    static kSecClassGenericPassword: CFStringRef;
    static kSecAttrService: CFStringRef;
    static kSecAttrAccount: CFStringRef;
    static kSecReturnAttributes: CFStringRef;
    static kSecReturnPersistentRef: CFStringRef;
    static kSecReturnData: CFStringRef;
    static kSecMatchLimit: CFStringRef;
    static kSecMatchLimitAll: CFStringRef;
    static kSecValuePersistentRef: CFStringRef;
    static kSecValueData: CFStringRef;
    fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    fn SecItemAdd(query: CFDictionaryRef, result: *mut CFTypeRef) -> i32;
    fn SecItemUpdate(query: CFDictionaryRef, attributes: CFDictionaryRef) -> i32;
    fn SecItemDelete(query: CFDictionaryRef) -> i32;
    fn SecKeychainCopyDefault(keychain: *mut *const c_void) -> i32;
    fn SecKeychainGetStatus(keychain: *const c_void, status: *mut u32) -> i32;
}
fn key(value: CFStringRef) -> CFString {
    unsafe { CFString::wrap_under_get_rule(value) }
}
fn error(code: i32) -> String {
    format!("Credential storage unavailable, denied or locked (Security OSStatus {code})")
}
fn check(code: i32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(error(code))
    }
}
fn base() -> Vec<(CFString, CFType)> {
    unsafe {
        vec![
            (key(kSecClass), key(kSecClassGenericPassword).as_CFType()),
            (
                key(kSecAttrService),
                CFString::new("io.local.ongleet.credentials").as_CFType(),
            ),
        ]
    }
}
fn dict(pairs: &[(CFString, CFType)]) -> CFDictionary<CFString, CFType> {
    CFDictionary::from_CFType_pairs(pairs)
}
fn matching(pairs: &[(CFString, CFType)]) -> Result<Option<CFType>, String> {
    let query = dict(pairs);
    let mut result = std::ptr::null();
    let code = unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result) };
    if code == -25300 {
        return Ok(None);
    }
    check(code)?;
    if result.is_null() {
        return Err("Keychain returned no result".into());
    }
    Ok(Some(unsafe { CFType::wrap_under_create_rule(result) }))
}
fn truth() -> CFType {
    core_foundation::boolean::CFBoolean::true_value().as_CFType()
}
fn encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}
fn reference(record: &SecretRecord) -> Result<CFData, String> {
    if record.identity.len() % 2 != 0 {
        return Err("Invalid keychain reference".into());
    }
    let bytes = (0..record.identity.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&record.identity[i..i + 2], 16)
                .map_err(|_| "Invalid keychain reference".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CFData::from_buffer(&bytes))
}
fn available() -> Result<(), String> {
    let mut raw = std::ptr::null();
    check(unsafe { SecKeychainCopyDefault(&mut raw) })?;
    if raw.is_null() {
        return Err("Default keychain unavailable".into());
    }
    let owner = unsafe { CFType::wrap_under_create_rule(raw) };
    let mut status = 0;
    check(unsafe { SecKeychainGetStatus(owner.as_CFTypeRef(), &mut status) })?;
    if status & 1 == 0 {
        return Err("Default keychain is locked".into());
    }
    Ok(())
}
impl SecretStore for super::Macos {
    fn ensure_available(&self) -> SystemFuture<'_, ()> {
        Box::pin(async { available() })
    }
    fn search<'a>(&'a self, attributes: &'a SecretAttributes) -> SystemFuture<'a, SecretSearch> {
        Box::pin(async move {
            available()?;
            let mut query = base();
            unsafe {
                query.extend([
                    (key(kSecReturnAttributes), truth()),
                    (key(kSecReturnPersistentRef), truth()),
                    (key(kSecMatchLimit), key(kSecMatchLimitAll).as_CFType()),
                ]);
            }
            let mut records = Vec::new();
            if let Some(result) = matching(&query)? {
                let raw_array = result
                    .downcast::<CFArray>()
                    .ok_or("Unexpected keychain search result")?;
                let array = unsafe {
                    CFArray::<CFDictionary<CFString, CFType>>::wrap_under_get_rule(
                        raw_array.as_concrete_TypeRef(),
                    )
                };
                for item in array.iter() {
                    let account = item
                        .find(unsafe { &key(kSecAttrAccount) })
                        .ok_or("Keychain account missing")?
                        .downcast::<CFString>()
                        .ok_or("Invalid keychain account")?;
                    let stored: SecretAttributes = serde_json::from_str(&account.to_string())
                        .map_err(|_| "Invalid keychain attributes")?;
                    if !attributes.iter().all(|(k, v)| stored.get(k) == Some(v)) {
                        continue;
                    }
                    let persistent = item
                        .find(unsafe { &key(kSecValuePersistentRef) })
                        .ok_or("Persistent keychain reference missing")?
                        .downcast::<CFData>()
                        .ok_or("Invalid persistent reference")?;
                    records.push(SecretRecord {
                        identity: encode(persistent.bytes()),
                        attributes: stored,
                    });
                }
            }
            Ok(SecretSearch {
                locked: false,
                records,
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
            available()?;
            let mut query = base();
            query.push((
                unsafe { key(kSecAttrAccount) },
                CFString::new(&serde_json::to_string(attributes).map_err(|e| e.to_string())?)
                    .as_CFType(),
            ));
            let data = (
                unsafe { key(kSecValueData) },
                CFData::from_buffer(bytes).as_CFType(),
            );
            if replace {
                let code = unsafe {
                    SecItemUpdate(
                        dict(&query).as_concrete_TypeRef(),
                        dict(&[data.clone()]).as_concrete_TypeRef(),
                    )
                };
                if code == 0 {
                    return Ok(());
                }
                if code != -25300 {
                    return Err(error(code));
                }
            }
            query.push(data);
            check(unsafe { SecItemAdd(dict(&query).as_concrete_TypeRef(), std::ptr::null_mut()) })
        })
    }
    fn read<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, Vec<u8>> {
        Box::pin(async move {
            available()?;
            let query = unsafe {
                vec![
                    (key(kSecValuePersistentRef), reference(record)?.as_CFType()),
                    (key(kSecReturnData), truth()),
                ]
            };
            matching(&query)?
                .ok_or("Credential no longer exists")?
                .downcast::<CFData>()
                .map(|data| data.bytes().to_vec())
                .ok_or_else(|| "Invalid credential data".into())
        })
    }
    fn remove<'a>(&'a self, record: &'a SecretRecord) -> SystemFuture<'a, ()> {
        Box::pin(async move {
            available()?;
            let query =
                unsafe { vec![(key(kSecValuePersistentRef), reference(record)?.as_CFType())] };
            let code = unsafe { SecItemDelete(dict(&query).as_concrete_TypeRef()) };
            if code == -25300 {
                Ok(())
            } else {
                check(code)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::SystemFiles;
    use super::*;
    extern "C" {
        fn SecKeychainGetPath(keychain: *const c_void, length: *mut u32, path: *mut i8) -> i32;
        fn SecKeychainLock(keychain: *const c_void) -> i32;
        fn SecKeychainUnlock(
            keychain: *const c_void,
            length: u32,
            password: *const c_void,
            use_password: bool,
        ) -> i32;
    }
    #[test]
    #[ignore = "requires an explicitly selected disposable default keychain in a dedicated macOS test account"]
    fn isolated_keychain_persistent_refs_replacement_and_lock_errors() {
        let expected = std::env::var("ONGLEET_TEST_KEYCHAIN")
            .expect("Explicit disposable keychain path required");
        let password = std::env::var("ONGLEET_TEST_KEYCHAIN_PASSWORD")
            .expect("Disposable keychain password required");
        let mut raw = std::ptr::null();
        check(unsafe { SecKeychainCopyDefault(&mut raw) }).unwrap();
        let owner = unsafe { CFType::wrap_under_create_rule(raw) };
        let mut path = [0i8; 4096];
        let mut length = path.len() as u32;
        check(unsafe { SecKeychainGetPath(raw, &mut length, path.as_mut_ptr()) }).unwrap();
        let actual = unsafe { std::ffi::CStr::from_ptr(path.as_ptr()) }
            .to_str()
            .unwrap();
        assert_eq!(
            std::fs::canonicalize(actual).unwrap(),
            std::fs::canonicalize(expected).unwrap(),
            "Refusing to alter an unrelated keychain"
        );
        struct Unlock {
            keychain: CFType,
            password: String,
        }
        impl Drop for Unlock {
            fn drop(&mut self) {
                unsafe {
                    SecKeychainUnlock(
                        self.keychain.as_CFTypeRef(),
                        self.password.len() as u32,
                        self.password.as_ptr().cast(),
                        true,
                    )
                };
            }
        }
        let unlock = Unlock {
            keychain: owner,
            password,
        };
        let native = super::super::Macos::new();
        let plugin = format!("macos-fixture-{}", native.nonce().unwrap());
        let attrs = SecretAttributes::from([
            ("application".into(), "ongleet".into()),
            ("plugin".into(), plugin.clone()),
            ("key".into(), "account".into()),
        ]);
        tauri::async_runtime::block_on(async {
            native.ensure_available().await.unwrap();
            native.create(&attrs, b"first", false).await.unwrap();
            let found = native
                .search(&SecretAttributes::from([("plugin".into(), plugin)]))
                .await
                .unwrap();
            assert_eq!(found.records.len(), 1);
            let record = &found.records[0];
            assert!(!record.identity.is_empty());
            assert_eq!(native.read(record).await.unwrap(), b"first");
            assert!(native.create(&attrs, b"duplicate", false).await.is_err());
            native.create(&attrs, b"replacement", true).await.unwrap();
            assert_eq!(native.read(record).await.unwrap(), b"replacement");
            check(unsafe { SecKeychainLock(raw) }).unwrap();
            assert!(native.ensure_available().await.is_err());
            assert!(native.search(&attrs).await.is_err());
            assert!(native.read(record).await.is_err());
            check(unsafe {
                SecKeychainUnlock(
                    raw,
                    unlock.password.len() as u32,
                    unlock.password.as_ptr().cast(),
                    true,
                )
            })
            .unwrap();
            native.remove(record).await.unwrap();
            assert!(native.search(&attrs).await.unwrap().records.is_empty());
        });
    }
}
