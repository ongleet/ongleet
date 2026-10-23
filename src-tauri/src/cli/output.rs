use crate::control::protocol::{Response, RpcError};
use serde_json::Value;

/// Keep the wire response intact for automation; presentation belongs to the CLI.
pub(super) fn success(response: &Response, json: bool) -> String {
    if json {
        return serde_json::to_string(response).unwrap();
    }
    match response.data.as_ref() {
        Some(value) if !value.is_null() => {
            let mut text = String::new();
            render(value, 0, &mut text);
            text.trim_end().to_owned()
        }
        _ => "Done.".into(),
    }
}

pub(super) fn error(error: RpcError, json: bool) -> String {
    if json {
        serde_json::to_string(&Response::failure(error)).unwrap()
    } else {
        format!("Error ({}): {}", clean(&error.code), clean(&error.message))
    }
}

// Prevent names, paths and remote diagnostic text from injecting terminal controls.
fn clean(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn label(key: &str) -> &str {
    match key {
        "id" => "ID",
        "name" => "Name",
        "kind" => "Type",
        "selectedId" => "Selected session",
        "selectedTabId" => "Selected tab",
        "sessionId" => "Session ID",
        "projectId" => "Project ID",
        "rootDirectory" => "Root directory",
        "baseDirectory" => "Base directory",
        "displayOrder" => "Display order",
        "runningTerminals" => "Running terminals",
        "configVersion" => "Configuration version",
        "resumeOnStart" => "Restart on startup",
        "isOpen" => "Open",
        _ => key,
    }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("none".into()),
        Value::String(value) => Some(clean(value)),
        Value::Bool(value) => Some(if *value { "yes" } else { "no" }.into()),
        Value::Number(value) => Some(value.to_string()),
        Value::Array(values) if values.is_empty() => Some("(empty)".into()),
        Value::Object(values) if values.is_empty() => Some("(empty)".into()),
        _ => None,
    }
}

fn render(value: &Value, indent: usize, text: &mut String) {
    let padding = " ".repeat(indent);
    if let Some(value) = scalar(value) {
        text.push_str(&format!("{padding}{value}\n"));
        return;
    }
    match value {
        Value::Object(fields) => {
            // Identity comes first, regardless of the JSON object's key order.
            for key in ["id", "name", "kind"] {
                if let Some(value) = fields.get(key) {
                    field(key, value, indent, text);
                }
            }
            for (key, value) in fields {
                if !matches!(key.as_str(), "id" | "name" | "kind") {
                    field(key, value, indent, text);
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                if let Some(value) = scalar(value) {
                    text.push_str(&format!("{padding}- {value}\n"));
                } else {
                    text.push_str(&format!("{padding}[{}]\n", index + 1));
                    render(value, indent + 2, text);
                }
            }
        }
        _ => unreachable!(),
    }
}

fn field(key: &str, value: &Value, indent: usize, text: &mut String) {
    let padding = " ".repeat(indent);
    let label = clean(&label(key));
    if let Some(value) = scalar(value) {
        text.push_str(&format!("{padding}{label}: {value}\n"));
    } else {
        text.push_str(&format!("{padding}{label}:\n"));
        render(value, indent + 2, text);
    }
}
