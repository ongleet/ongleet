//! Bounded attachment-to-attachment request routing. Providers remain JavaScript.
use super::commands::SdkError;
use crate::{session::SessionRegistry, AppRegistry};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{Emitter, State};

type Reply = Result<Value, String>;
struct Pending {
    target: u64,
    attachment: String,
    caller: Option<u64>,
    output: Value,
    sender: mpsc::Sender<Reply>,
}
#[derive(Default)]
pub struct Broker {
    next: u64,
    providers: HashMap<u64, (String, bool, Vec<String>)>,
    pending: HashMap<u64, Pending>,
}
impl Broker {
    fn reserve(
        &mut self,
        target: u64,
        attachment: String,
        caller: Option<u64>,
        output: Value,
    ) -> Result<(u64, mpsc::Receiver<Reply>), String> {
        if self.pending.len() >= 32
            || self.pending.values().filter(|p| p.target == target).count() >= 4
        {
            return Err("Interaction concurrency limit exceeded".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("Request ID limit exceeded")?;
        let (sender, receiver) = mpsc::channel();
        self.pending.insert(
            self.next,
            Pending {
                target,
                attachment,
                caller,
                output,
                sender,
            },
        );
        Ok((self.next, receiver))
    }
    fn reply(
        &mut self,
        id: u64,
        attachment: &str,
        request_id: u64,
        result: Value,
        error: Option<String>,
    ) -> Result<(), String> {
        let pending = self
            .pending
            .get(&request_id)
            .ok_or("Request unavailable: late or duplicate reply")?;
        if pending.target != id || pending.attachment != attachment {
            return Err("Reply attachment unavailable".into());
        }
        let reply = if let Some(error) = error {
            bounded(&Value::String(error.clone()))
                .map(|_| Value::Null)
                .and(Err(error))
        } else {
            validate(&result, &pending.output).map(|_| result)
        };
        let pending = self.pending.remove(&request_id).unwrap();
        let _ = pending.sender.send(reply);
        Ok(())
    }
    pub fn disconnect(&mut self) {
        self.providers.clear();
        for (_, pending) in self.pending.drain() {
            let _ = pending
                .sender
                .send(Err("Frontend unavailable: disconnected".into()));
        }
    }
    pub fn detach(&mut self, id: u64) {
        self.providers.remove(&id);
        self.pending.retain(|_, p| {
            if p.target == id || p.caller == Some(id) {
                let _ = p
                    .sender
                    .send(Err("Interaction unavailable: attachment detached".into()));
                false
            } else {
                true
            }
        });
    }
}
fn bounded(value: &Value) -> Result<(), String> {
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 65_536 {
        Err("Interaction exceeds 64 KiB limit".into())
    } else {
        Ok(())
    }
}
fn validate(value: &Value, schema: &Value) -> Result<(), String> {
    bounded(value)?;
    crate::plugins::validate_schema(value, schema).map_err(|e| format!("invalid_arguments: {e}"))
}
fn authorize(registry: &SessionRegistry, caller: Option<(u64, &str)>) -> Result<(), String> {
    if let Some((id, attachment)) = caller {
        if registry.sdk_attachments.get(&id).map(String::as_str) != Some(attachment) {
            return Err("Caller attachment unavailable".into());
        }
        let tab = registry.open_plugin_tab(id)?;
        if !crate::plugins::manifest(&tab.kind).is_some_and(|m| {
            m["capabilities"]
                .as_array()
                .is_some_and(|c| c.iter().any(|v| v == "tabs"))
        }) {
            return Err("Required tabs capability unavailable".into());
        }
    }
    Ok(())
}
pub fn execute(
    app: &tauri::AppHandle,
    shared: &Arc<Mutex<SessionRegistry>>,
    caller: Option<(u64, &str)>,
    id: u64,
    operation: &str,
    args: Value,
) -> Reply {
    let started = Instant::now();
    let (request_id, attachment, receiver, identity) = {
        let mut registry = shared.lock().map_err(|e| e.to_string())?;
        authorize(&registry, caller)?;
        let snapshot = registry.snapshot();
        if operation == "list" {
            validate(
                &args,
                &json!({"type":"object","additionalProperties":false,"properties":{"sessionId":{"type":"integer","minimum":1}}}),
            )?;
            let tabs: Vec<_> = snapshot.sessions.iter().filter(|s| args["sessionId"].as_u64().is_none_or(|id| s.id == id)).flat_map(|s| s.tabs.iter().map(move |t| json!({"id":t.id,"sessionId":s.id,"kind":t.kind,"name":t.name,"isOpen":t.is_open}))).collect();
            return Ok(json!({"tabs": tabs}));
        }
        let session = snapshot
            .sessions
            .iter()
            .find(|s| s.tabs.iter().any(|t| t.id == id))
            .ok_or("Tab unavailable")?;
        let tab = session.tabs.iter().find(|t| t.id == id).unwrap();
        let identity = json!({"id":id,"sessionId":session.id,"kind":tab.kind,"name":tab.name});
        let manifest = crate::plugins::manifest(&tab.kind).unwrap_or(Value::Null);
        let descriptor = &manifest["interactions"];
        let provider = registry.interactions.providers.get(&id);
        if operation == "capabilities" {
            return Ok(
                json!({"tab":identity,"interactions":descriptor,"available":tab.is_open && provider.is_some(),"stateAvailable":tab.is_open && provider.is_some_and(|p| p.1),"availableActions":provider.map(|p| p.2.clone()).unwrap_or_default()}),
            );
        }
        if !tab.is_open {
            return Err("Tab unavailable: closed".into());
        }
        if descriptor.is_null() || descriptor["version"] != 1 {
            return Err("unsupported_operation: Plugin declares no interactions".into());
        }
        let declaration = if operation == "state" {
            &descriptor["state"]
        } else {
            &descriptor["actions"][operation]
        };
        if declaration.is_null() {
            return Err("unsupported_operation: Operation is not declared".into());
        }
        validate(&args, &declaration["inputSchema"])?;
        if !tab.is_open {
            return Err("Tab unavailable: closed".into());
        }
        let provider = provider.ok_or("Interaction provider unavailable")?;
        if (operation == "state" && !provider.1)
            || (operation != "state" && !provider.2.iter().any(|a| a == operation))
        {
            return Err("Interaction handler unavailable".into());
        }
        let attachment = provider.0.clone();
        if registry.sdk_attachments.get(&id) != Some(&attachment) {
            return Err("Target attachment unavailable".into());
        }
        let (request_id, receiver) = registry.interactions.reserve(
            id,
            attachment.clone(),
            caller.map(|c| c.0),
            declaration["outputSchema"].clone(),
        )?;
        (request_id, attachment, receiver, identity)
    };
    let event = format!("tab-interaction-{id}-{attachment}");
    let sent = app.emit(
        &event,
        json!({"requestId":request_id,"operation":operation,"args":args}),
    );
    let result = if let Err(error) = sent {
        Err(format!("Frontend unavailable: {error}"))
    } else {
        receiver.recv_timeout(Duration::from_secs(5)).unwrap_or_else(|e| Err(match e {
            mpsc::RecvTimeoutError::Timeout => "Interaction timed out; an action may have executed. Do not automatically retry.".into(),
            _ => "Interaction unavailable: disconnected".into(),
        }))
    };
    shared
        .lock()
        .map_err(|e| e.to_string())?
        .interactions
        .pending
        .remove(&request_id);
    let _ = app.emit(&event, json!({"cancel":request_id}));
    let result = result.and_then(|value| {
        if operation == "state" {
            validate(&value, &json!({"type":"object","additionalProperties":false,"required":["summary","data","observedAt","stale"],"properties":{"summary":{"type":"string"},"data":{},"observedAt":{"type":["string","null"]},"stale":{"type":"boolean"}}}))?;
        }
        Ok(value)
    });
    result.map(|value| {
        if operation == "state" {
            json!({"tab":identity,"state":value,"responseMs":started.elapsed().as_millis()})
        } else {
            value
        }
    })
}
#[tauri::command]
pub async fn sdk_interaction(
    app: tauri::AppHandle,
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    target: u64,
    operation: String,
    args: Value,
) -> Result<Value, SdkError> {
    let shared = state.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute(
            &app,
            &shared,
            Some((id, &attachment)),
            target,
            &operation,
            args,
        )
    })
    .await
    .map_err(|e| SdkError::from(e.to_string()))?
    .map_err(SdkError::from)
}
#[tauri::command]
pub fn sdk_interaction_register(
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    enabled: bool,
    has_state: bool,
    actions: Vec<String>,
) -> Result<(), SdkError> {
    let mut registry = state.0.lock().map_err(|e| SdkError::from(e.to_string()))?;
    if registry.sdk_attachments.get(&id) != Some(&attachment) {
        return Err(SdkError::from("Attachment unavailable".to_string()));
    }
    let tab = registry.open_plugin_tab(id).map_err(SdkError::from)?;
    let manifest = crate::plugins::manifest(&tab.kind).unwrap_or(Value::Null);
    let descriptor = &manifest["interactions"];
    if enabled
        && (descriptor["version"] != 1
            || (has_state && descriptor["state"].is_null())
            || actions.iter().any(|a| {
                a == "state"
                    || a == "capabilities"
                    || a == "list"
                    || descriptor["actions"][a].is_null()
            }))
    {
        return Err(SdkError::from(
            "unsupported_operation: Registration does not match manifest".to_string(),
        ));
    }
    registry.interactions.detach(id);
    if enabled {
        registry
            .interactions
            .providers
            .insert(id, (attachment, has_state, actions));
    }
    Ok(())
}
#[tauri::command]
pub fn sdk_interaction_reply(
    state: State<'_, AppRegistry>,
    id: u64,
    attachment: String,
    request_id: u64,
    result: Value,
    error: Option<String>,
) -> Result<(), SdkError> {
    let mut registry = state.0.lock().map_err(|e| SdkError::from(e.to_string()))?;
    if registry.sdk_attachments.get(&id) != Some(&attachment) {
        return Err(SdkError::from("Attachment unavailable".to_string()));
    }
    registry
        .interactions
        .reply(id, &attachment, request_id, result, error)
        .map_err(SdkError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reserve(
        broker: &mut Broker,
        target: u64,
        caller: Option<u64>,
    ) -> (u64, mpsc::Receiver<Reply>) {
        broker
            .reserve(
                target,
                "attachment".into(),
                caller,
                json!({"type":"integer"}),
            )
            .unwrap()
    }
    #[test]
    fn replies_require_target_attachment_and_are_single_use() {
        let mut broker = Broker::default();
        let (one, first) = reserve(&mut broker, 42, Some(7));
        let (two, second) = reserve(&mut broker, 42, Some(8));
        assert!(broker.reply(99, "attachment", one, json!(1), None).is_err());
        assert!(broker.reply(42, "old", one, json!(1), None).is_err());
        assert!(first.try_recv().is_err());
        broker.reply(42, "attachment", two, json!(2), None).unwrap();
        broker.reply(42, "attachment", one, json!(1), None).unwrap();
        assert_eq!(first.recv().unwrap().unwrap(), json!(1));
        assert_eq!(second.recv().unwrap().unwrap(), json!(2));
        assert!(broker
            .reply(42, "attachment", one, json!(99), None)
            .is_err());
    }
    #[test]
    fn detach_cancels_caller_and_target_without_touching_siblings() {
        let mut broker = Broker::default();
        let (one, target) = reserve(&mut broker, 42, Some(7));
        let (_, caller) = reserve(&mut broker, 8, Some(42));
        let (three, sibling) = reserve(&mut broker, 8, Some(9));
        broker.detach(42);
        assert!(target.recv().unwrap().unwrap_err().contains("detached"));
        assert!(caller.recv().unwrap().unwrap_err().contains("detached"));
        assert!(broker.reply(42, "attachment", one, json!(1), None).is_err());
        broker
            .reply(8, "attachment", three, json!(3), None)
            .unwrap();
        assert_eq!(sibling.recv().unwrap().unwrap(), json!(3));
    }
    #[test]
    fn concurrency_limits_and_failed_results_release_capacity() {
        let mut broker = Broker::default();
        let mut receivers = vec![];
        for target in 0..8 {
            for _ in 0..4 {
                receivers.push(reserve(&mut broker, target, None));
            }
        }
        assert!(broker.reserve(9, "a".into(), None, json!({})).is_err());
        let (id, receiver) = receivers.remove(0);
        broker
            .reply(0, "attachment", id, json!("wrong result"), None)
            .unwrap();
        assert!(receiver
            .recv()
            .unwrap()
            .unwrap_err()
            .starts_with("invalid_arguments"));
        assert!(broker.reserve(1, "a".into(), None, json!({})).is_err());
        assert!(broker.reserve(0, "a".into(), None, json!({})).is_ok());
    }
    #[test]
    fn reload_cancels_pending_requests_and_never_reuses_request_ids() {
        let mut broker = Broker::default();
        let (old, receiver) = reserve(&mut broker, 42, None);
        broker
            .providers
            .insert(42, ("attachment".into(), true, vec![]));
        broker.disconnect();
        assert!(receiver
            .recv()
            .unwrap()
            .unwrap_err()
            .contains("disconnected"));
        assert!(broker.providers.is_empty());
        let (new, receiver) = reserve(&mut broker, 42, None);
        assert_ne!(old, new);
        assert!(broker.reply(42, "attachment", old, json!(9), None).is_err());
        broker.reply(42, "attachment", new, json!(2), None).unwrap();
        assert_eq!(receiver.recv().unwrap().unwrap(), json!(2));
    }
    #[test]
    fn schemas_reject_unknown_missing_wrong_and_out_of_range_arguments() {
        let manifest = crate::plugins::manifest("terminal").unwrap();
        let state = &manifest["interactions"]["state"]["inputSchema"];
        for value in [
            json!({"lines":501}),
            json!({"lines":0}),
            json!({"lines":1.5}),
            json!({"lines":"10"}),
            json!({"unknown":true}),
        ] {
            assert!(validate(&value, state).is_err());
        }
        assert!(validate(&json!({}), state).is_ok());
        assert!(validate(&json!({"lines":500}), state).is_ok());
        let input = &manifest["interactions"]["actions"]["send-text"]["inputSchema"];
        assert!(validate(&json!({}), input).is_err());
        assert!(validate(&json!({"text":"日".repeat(3000)}), input).is_err());
        assert!(bounded(&json!("x".repeat(65_536))).is_err());
        let mut broker = Broker::default();
        let (id, receiver) = broker.reserve(1, "a".into(), None, json!({})).unwrap();
        broker
            .reply(1, "a", id, json!("x".repeat(65_536)), None)
            .unwrap();
        assert!(receiver.recv().unwrap().unwrap_err().contains("64 KiB"));
    }
}
