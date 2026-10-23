//! Declarative registration for trusted, bundled JavaScript plugins.
mod catalog;
pub use catalog::catalog;

include!(concat!(env!("OUT_DIR"), "/plugin_manifests.rs"));
pub fn manifests() -> Vec<serde_json::Value> {
    MANIFESTS
        .iter()
        .map(|text| serde_json::from_str(text).expect("bundled manifest"))
        .collect()
}
pub fn manifest(kind: &str) -> Option<serde_json::Value> {
    manifests()
        .into_iter()
        .find(|m| m["id"] == kind && m["sdkVersion"] == 1)
}
pub fn available(kind: &str, version: u32) -> bool {
    manifest(kind).is_some_and(|m| m["configVersion"] == version)
}
pub fn has_runtime(kind: &str) -> bool {
    manifest(kind).is_some_and(|m| m["runtime"] == "pty")
}
pub fn validate(kind: &str, version: u32, config: &serde_json::Value) -> Result<(), String> {
    let manifest = manifest(kind).ok_or_else(|| format!("Plugin unavailable: {kind}"))?;
    if manifest["configVersion"] != version {
        return Err(format!(
            "Unsupported {kind} configuration version {version}"
        ));
    }
    if serde_json::to_vec(config).map_err(|e| e.to_string())?.len() > 65_536 {
        return Err("Plugin configuration exceeds 64 KiB".into());
    }
    validate_schema(config, &manifest["configSchema"])?;
    // Runtime launch requests always receive the core's full semantic validation.
    if has_runtime(kind) {
        serde_json::from_value::<crate::core::pty::TabConfig>(config.clone())
            .map_err(|e| e.to_string())?
            .validate()?;
    }
    Ok(())
}
fn validate_schema(value: &serde_json::Value, schema: &serde_json::Value) -> Result<(), String> {
    use serde_json::Value;
    let matches = |name: &str| match name {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        _ => false,
    };
    let valid = match &schema["type"] {
        Value::String(t) => matches(t),
        Value::Array(ts) => ts.iter().any(|t| t.as_str().is_some_and(matches)),
        Value::Null => true,
        _ => false,
    };
    if !valid {
        return Err("Invalid configuration value type".into());
    }
    if let Some(object) = value.as_object() {
        for (key, child) in object {
            if let Some(property) = schema["properties"].get(key) {
                validate_schema(child, property).map_err(|e| format!("{key}: {e}"))?;
            } else if schema["additionalProperties"] == false {
                return Err(format!("Unknown configuration field: {key}"));
            } else if schema["additionalProperties"].is_object() {
                validate_schema(child, &schema["additionalProperties"])?;
            }
        }
    }
    if let Some(items) = value.as_array() {
        for child in items {
            validate_schema(child, &schema["items"])?;
        }
    }
    if let Some(text) = value.as_str() {
        if schema["absolutePath"] == true
            && (!std::path::Path::new(text).is_absolute() || text.contains('\0'))
        {
            return Err("Expected an absolute path without NUL".into());
        }
        if schema["maxBytes"]
            .as_u64()
            .is_some_and(|n| text.len() as u64 > n)
        {
            return Err("Configuration string exceeds byte limit".into());
        }
    }
    Ok(())
}
pub fn initialize(
    kind: &str,
    mut config: serde_json::Value,
    directory: &str,
) -> Result<serde_json::Value, String> {
    let manifest = manifest(kind).ok_or("Plugin unavailable")?;
    if let Some(defaults) = manifest["defaults"].as_object() {
        let object = config
            .as_object_mut()
            .ok_or("Plugin configuration must be an object")?;
        for (key, value) in defaults {
            object.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    if let Some(defaults) = manifest["contextDefaults"].as_object() {
        for (key, source) in defaults {
            if config[key].is_null() && source == "session.directory" {
                config[key] = directory.into();
            }
        }
    }
    Ok(config)
}
#[derive(Default)]
pub struct Job {
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub finished: std::sync::atomic::AtomicBool,
}
