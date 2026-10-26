//! Read-only access to the native app's launch environment for trusted plugins.
// Names and values never enter diagnostic messages. No variable whitelist yet.
pub fn get(name: &str) -> Result<serde_json::Value, String> {
    read(name, |name| std::env::var_os(name))
}
fn read(
    name: &str,
    lookup: impl FnOnce(&str) -> Option<std::ffi::OsString>,
) -> Result<serde_json::Value, String> {
    if name.is_empty() || name.len() > 1024 || name.contains(['\0', '=']) {
        return Err("Invalid environment variable name".into());
    }
    match lookup(name) {
        None => Ok(serde_json::Value::Null),
        Some(value) => {
            let value = value
                .into_string()
                .map_err(|_| "Environment value is not UTF-8")?;
            if value.len() > 65_536 {
                return Err("Environment value exceeds 64 KiB".into());
            }
            Ok(value.into())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    #[test]
    fn distinguishes_unset_and_empty_without_restricting_variable_names() {
        assert_eq!(
            read("CUSTOM_TOKEN", |_| None).unwrap(),
            serde_json::Value::Null
        );
        assert_eq!(
            read("CUSTOM_TOKEN", |_| Some(OsString::from(""))).unwrap(),
            ""
        );
        assert_eq!(
            read("GH_TOKEN", |_| Some(OsString::from("secret"))).unwrap(),
            "secret"
        );
    }
    #[test]
    fn rejects_invalid_names_and_unbounded_or_non_utf8_values() {
        use std::os::unix::ffi::OsStringExt;
        for name in ["", "A=B", "A\0B"] {
            assert!(read(name, |_| panic!("invalid names must not be read")).is_err());
        }
        assert!(read("TOKEN", |_| Some(OsString::from("x".repeat(65_537))))
            .unwrap_err()
            .contains("64 KiB"));
        assert!(read("TOKEN", |_| Some(OsString::from_vec(vec![255])))
            .unwrap_err()
            .contains("UTF-8"));
    }
}
