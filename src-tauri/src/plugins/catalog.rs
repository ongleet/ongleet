//! Agent-facing discovery; examples are checked against the real plugin validators.
use serde_json::{json, Value};

pub fn catalog() -> Value {
    json!({
        "types": [
            {
                "kind": "terminal", "configVersion": 1,
                "description": "A contained PTY process. Inherits the normal process environment; stores explicit overrides only.",
                "configSchema": {
                    "type": "object", "additionalProperties": false,
                    "properties": {
                        "cwdOverride": {"type": ["string", "null"], "description": "Absolute working directory. Omitted/null uses the session directory."},
                        "executable": {"type": ["string", "null"], "description": "Executable path or PATH name. Omitted/null uses the user shell."},
                        "args": {"type": "array", "items": {"type": "string"}, "default": [], "description": "Literal argument array; no shell expansion."},
                        "env": {"type": "object", "additionalProperties": {"type": "string"}, "default": {}, "description": "Explicit environment overrides."},
                        "shellCommand": {"type": ["string", "null"], "description": "Explicit shell syntax; cannot be combined with executable or nonempty args."}
                    }
                },
                "examples": [
                    {"executable": "/bin/bash", "args": [], "env": {}},
                    {"executable": "/usr/bin/python3", "args": ["-c", "print('hello')"], "env": {}},
                    {"shellCommand": "printf '%s\n' hello", "env": {}}
                ],
                "notes": ["Commands and arguments cannot contain NUL bytes. Explicit executable and shellCommand must be nonempty.",
                    "cwdOverride must be absolute; the launch directory must exist.",
                    "Environment keys must be nonempty and contain neither '=' nor NUL; values cannot contain NUL.",
                    "Configuration changes affect future launches; running processes retain their launch settings."]
            },
            {
                "kind": "files", "configVersion": 1,
                "description": "Read-only file browser.",
                "configSchema": {
                    "type": "object", "additionalProperties": false,
                    "properties": {"folder": {"type": ["string", "null"], "description": "Absolute path, at most 8192 bytes, no NUL. Defaults to session directory on creation."}}
                },
                "examples": [{}, {"folder": "/tmp"}],
                "notes": ["Browsing requires an accessible directory. Configuration does not create directories."]
            },
            {
                "kind": "pull-request", "configVersion": 1,
                "description": "Read-only GitHub PR status, using authenticated gh and the session's explicit PR association.",
                "configSchema": {"type": "object", "additionalProperties": false, "properties": {}},
                "examples": [{}],
                "notes": ["Repository and PR number belong to the session association, not tab configuration.",
                    "Set the association in the Pull request UI; CLI association editing is not implemented."]
            }
        ],
        "configuration": {
            "mode": "replace",
            "description": "config is the plugin object itself, without a kind/configVersion wrapper. Unknown fields are rejected. Omitted fields use plugin defaults; this is not a patch.",
            "limits": {"pluginConfigBytes": 65536, "socketRequestBytes": 16384},
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
    fn advertised_examples_validate_and_fields_match_serialized_configuration() {
        let catalog = super::catalog();
        for kind in catalog["types"].as_array().unwrap() {
            for config in kind["examples"].as_array().unwrap() {
                crate::plugins::validate(kind["kind"].as_str().unwrap(), 1, config).unwrap();
            }
            let config = match kind["kind"].as_str().unwrap() {
                "terminal" => {
                    serde_json::to_value(crate::plugins::terminal::TabConfig::default()).unwrap()
                }
                "files" => serde_json::to_value(crate::plugins::files::Config::default()).unwrap(),
                "pull-request" => serde_json::json!({}),
                _ => panic!("Unknown advertised type"),
            };
            let mut actual: Vec<_> = config.as_object().unwrap().keys().collect();
            let mut advertised: Vec<_> = kind["configSchema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .collect();
            actual.sort();
            advertised.sort();
            assert_eq!(actual, advertised);
        }
    }
}
