//! Agent-facing discovery; examples are checked against the real plugin validators.
use serde_json::{json, Value};

fn manifests() -> Vec<Value> {
    super::manifests()
        .into_iter()
        .map(|mut manifest| {
            manifest["kind"] = manifest["id"].clone();
            manifest
        })
        .collect()
}
pub fn catalog() -> Value {
    json!({
        "types": manifests(),
        "configuration": {
            "mode": "replace",
            "description": "config is the plugin object itself, without a kind/configVersion wrapper. Unknown fields are rejected. Omitted fields use plugin defaults; this is not a patch.",
            "limits": {"pluginConfigBytes": 65536, "socketRequestBytes": crate::control::protocol::REQUEST_LIMIT},
            "notes": ["The complete encoded request must fit below the socket request limit.",
                "tab create --config replaces the action configuration before the first launch, including its explicit environment overrides.",
                "External command actions are not tab types and are excluded from tab actions."]
        },
        "workflow": [
            "devcenter session list",
            "devcenter tab types",
            "devcenter tab actions --session SESSION_ID",
            "devcenter tab create --session SESSION_ID --action ACTION_ID [--name NAME] [--config JSON]",
            "devcenter tab list --session SESSION_ID",
            "devcenter tab configure TAB_ID --config JSON"
        ]
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn advertised_examples_validate_and_unknown_fields_are_rejected() {
        let catalog = super::catalog();
        for kind in catalog["types"].as_array().unwrap() {
            for config in kind["examples"].as_array().unwrap() {
                crate::plugins::validate(kind["kind"].as_str().unwrap(), 1, config).unwrap();
            }
            assert!(kind["configSchema"]["additionalProperties"] == false);
            assert!(crate::plugins::validate(
                kind["kind"].as_str().unwrap(),
                1,
                &serde_json::json!({"unexpected":true})
            )
            .is_err());
        }
    }
}
