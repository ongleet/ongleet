use crate::{
    control::protocol::{Command, RpcError},
    session::{Session, SessionRegistry, Snapshot},
    terminal_session::Status,
    AppRegistry, Lifecycle,
};
use serde_json::{json, Value};
use tauri::{Emitter, Manager};

pub enum Effect {
    None,
    Show,
    Quit,
}
fn failed(error: String) -> RpcError {
    RpcError::new("operation_failed", error)
}
fn session(snapshot: &Snapshot, id: u64) -> Result<Session, RpcError> {
    snapshot
        .sessions
        .iter()
        .find(|w| w.id == id)
        .cloned()
        .ok_or_else(|| RpcError::new("not_found", "Session ID is unavailable"))
}
fn summary(registry: &mut SessionRegistry, session: &Session) -> Result<Value, RpcError> {
    let mut tabs = Vec::new();
    let mut running = 0;
    for tab in session.tabs.iter().filter(|tab| tab.is_open) {
        let status = if let Some(id) = registry.terminals.get(&tab.id).map(|session| session.id) {
            let status = registry.status(id).map_err(failed)?;
            if matches!(status, Status::Running) {
                running += 1;
            }
            serde_json::to_value(status).map_err(|e| failed(e.to_string()))?
        } else {
            json!({"state": if tab.kind != "terminal" { "open" } else if tab.resume_on_start { "starting" } else { "stopped" }})
        };
        tabs.push(json!({"id":tab.id,"name":tab.name,"kind":tab.kind,"status":status}));
    }
    Ok(
        json!({"id":session.id,"projectId":session.project_id,"name":session.name,"baseDirectory":session.base_directory,"displayOrder":session.display_order,"selectedTabId":session.selected_tab_id,"pullRequest":session.pull_request,"runningTerminals":running,"tabs":tabs}),
    )
}

pub fn handle(
    app: &tauri::AppHandle,
    command: Command,
) -> Result<(Option<Value>, Effect), RpcError> {
    if matches!(command, Command::Show) {
        return Ok((None, Effect::Show));
    }
    // Socket workers must not wait on GTK: application exit joins this worker.
    let lifecycle = app.state::<Lifecycle>();
    if lifecycle
        .quitting
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(RpcError::new("unavailable", "Application is shutting down"));
    }
    let visible = lifecycle.visible.load(std::sync::atomic::Ordering::Acquire);
    let shared = &app.state::<AppRegistry>().0;
    let mut registry = shared.lock().map_err(|e| failed(e.to_string()))?;
    let snapshot = registry.snapshot();
    let tab_id = match &command {
        Command::TabSelect { id }
        | Command::TabRename { id, .. }
        | Command::TabReorder { id, .. }
        | Command::TabOpen { id }
        | Command::TabClose { id, .. }
        | Command::TabDelete { id, .. }
        | Command::TabConfigure { id, .. } => Some(*id),
        _ => None,
    };
    if tab_id.is_some_and(|id| {
        !snapshot
            .sessions
            .iter()
            .flat_map(|s| &s.tabs)
            .any(|t| t.id == id)
    }) {
        return Err(RpcError::new("not_found", "Tab ID is unavailable"));
    }
    let mut changed = false;
    let mut effect = Effect::None;
    let data = match command {
        Command::Show => unreachable!(),
        Command::Status => {
            let mut running = 0;
            for session in registry.terminals.values_mut() {
                if matches!(session.status().map_err(failed)?, Status::Running) {
                    running += 1;
                }
            }
            json!({"pid":std::process::id(),"visible":visible,"projects":snapshot.projects.len(),"sessions":snapshot.sessions.len(),"runningTerminals":running})
        }
        Command::Quit { confirmed } => {
            if !confirmed {
                for session in registry.terminals.values_mut() {
                    if matches!(session.status().map_err(failed)?, Status::Running) {
                        return Err(RpcError::new(
                            "confirmation_required",
                            "Running processes require --confirm to quit",
                        ));
                    }
                }
            }
            registry.shutdown(confirmed).map_err(failed)?;
            app.state::<Lifecycle>()
                .quitting
                .store(true, std::sync::atomic::Ordering::Release);
            effect = Effect::Quit;
            json!({"quitting":true})
        }
        Command::SessionConfig { id } => {
            serde_json::to_value(session(&snapshot, id)?).map_err(|e| failed(e.to_string()))?
        }
        Command::SessionConfigure {
            id,
            name,
            directory,
            project_id,
        } => {
            if name.is_none() && directory.is_none() && project_id.is_none() {
                return Err(RpcError::new(
                    "invalid_argument",
                    "Supply a name, directory or project",
                ));
            }
            let old = session(&snapshot, id)?;
            let project_id = project_id.unwrap_or(old.project_id);
            if !snapshot.projects.iter().any(|p| p.id == project_id) {
                return Err(RpcError::new("not_found", "Project ID is unavailable"));
            }
            let next = registry
                .update_in_project(
                    id,
                    project_id,
                    name.unwrap_or(old.name),
                    directory.unwrap_or(old.base_directory),
                )
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(session(&next, id)?).map_err(|e| failed(e.to_string()))?
        }
        Command::SessionPullRequest { id, target, number } => {
            session(&snapshot, id)?;
            let association = if target == "none" {
                if number.is_some() {
                    return Err(RpcError::new(
                        "invalid_argument",
                        "--number cannot accompany --target none",
                    ));
                }
                None
            } else {
                Some(
                    crate::plugins::pull_request::Association::parse(target.trim(), number)
                        .map_err(|e| RpcError::new("invalid_argument", e))?,
                )
            };
            let next = registry
                .associate_session_pull_request(id, association)
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(session(&next, id)?).map_err(|e| failed(e.to_string()))?
        }
        Command::SessionList => {
            let mut list = Vec::new();
            for item in &snapshot.sessions {
                list.push(summary(&mut registry, item)?);
            }
            json!({"sessions":list,"selectedId":snapshot.selected_id})
        }
        Command::SessionCreate {
            project_id,
            name,
            directory,
        } => {
            if !snapshot.projects.iter().any(|g| g.id == project_id) {
                return Err(RpcError::new("not_found", "Project ID is unavailable"));
            }
            let directory_config = crate::control::paths::config_directory().map_err(failed)?;
            let next = registry
                .create_from_first_action(Some(project_id), name, directory, &directory_config)
                .map_err(failed)?;
            changed = true;
            summary(&mut registry, &session(&next, next.selected_id.unwrap())?)?
        }
        Command::SessionRename { id, name } => {
            let old = session(&snapshot, id)?;
            let next = registry
                .update(id, name, old.base_directory)
                .map_err(failed)?;
            changed = true;
            summary(&mut registry, &session(&next, id)?)?
        }
        Command::SessionDirectory { id, directory } => {
            let old = session(&snapshot, id)?;
            let next = registry.update(id, old.name, directory).map_err(failed)?;
            changed = true;
            summary(&mut registry, &session(&next, id)?)?
        }
        Command::SessionMove { id, project_id } => {
            let old = session(&snapshot, id)?;
            if !snapshot.projects.iter().any(|g| g.id == project_id) {
                return Err(RpcError::new("not_found", "Project ID is unavailable"));
            }
            let next = registry
                .update_in_project(id, project_id, old.name, old.base_directory)
                .map_err(failed)?;
            changed = true;
            summary(&mut registry, &session(&next, id)?)?
        }
        Command::SessionReorder { id, position } => {
            let old = session(&snapshot, id)?;
            let target = snapshot
                .sessions
                .iter()
                .filter(|w| w.project_id == old.project_id)
                .nth(position)
                .ok_or_else(|| {
                    RpcError::new(
                        "invalid_argument",
                        "Position is outside this project's session list",
                    )
                })?
                .display_order;
            let next = registry.reorder(id, target).map_err(failed)?;
            changed = true;
            summary(&mut registry, &session(&next, id)?)?
        }
        Command::SessionSelect { id } => {
            session(&snapshot, id)?;
            let next = registry.select(id).map_err(failed)?;
            changed = true;
            summary(&mut registry, &session(&next, id)?)?
        }
        Command::SessionClose { id, confirmed } => {
            let old = session(&snapshot, id)?;
            if summary(&mut registry, &old)?["runningTerminals"]
                .as_u64()
                .unwrap_or(0)
                > 0
                && !confirmed
            {
                return Err(RpcError::new(
                    "confirmation_required",
                    "Running session processes require --confirm to close",
                ));
            }
            // The registry independently preflights every session before signals.
            registry.close(id, confirmed).map_err(failed)?;
            changed = true;
            json!({"closed":id})
        }
        Command::TabTypes => crate::plugins::catalog(),
        Command::TabActions { session_id } => {
            let session = session(&snapshot, session_id)?;
            let directory = crate::control::paths::config_directory().map_err(failed)?;
            let actions: Vec<_> = crate::actions::load_project(&directory, session.project_id)
                .map_err(failed)?
                .into_iter()
                .filter(|action| crate::plugins::available(&action.kind, action.config_version))
                .collect();
            json!({"sessionId": session_id, "projectId": session.project_id, "actions": actions})
        }
        Command::TabList { session_id } => {
            let session = session(&snapshot, session_id)?;
            let observed = summary(&mut registry, &session)?;
            let mut tabs = Vec::new();
            for tab in &session.tabs {
                let mut value = serde_json::to_value(tab).map_err(|e| failed(e.to_string()))?;
                value["status"] = observed["tabs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["id"] == tab.id)
                    .map(|item| item["status"].clone())
                    .unwrap_or_else(|| json!({"state": "closed"}));
                tabs.push(value);
            }
            json!({"tabs": tabs, "selectedTabId": session.selected_tab_id})
        }
        Command::TabCreate {
            session_id,
            action_id,
            name,
            config,
        } => {
            let session = session(&snapshot, session_id)?;
            let directory = crate::control::paths::config_directory().map_err(failed)?;
            let action = crate::actions::load_project(&directory, session.project_id)
                .map_err(failed)?
                .into_iter()
                .find(|a| a.id == action_id)
                .ok_or_else(|| RpcError::new("not_found", "Action is unavailable"))?;
            let updated = registry
                .create_plugin_tab(
                    session_id,
                    name.unwrap_or(action.name),
                    action.kind,
                    action.config_version,
                    config.unwrap_or(action.config),
                )
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(updated).map_err(|e| failed(e.to_string()))?
        }
        Command::TabSelect { id } => {
            registry.select_tab(id).map_err(failed)?;
            changed = true;
            json!({"id": id})
        }
        Command::TabRename { id, name } => {
            registry.rename_tab(id, name).map_err(failed)?;
            changed = true;
            json!({"id": id})
        }
        Command::TabReorder { id, position } => {
            registry.reorder_tab(id, position).map_err(failed)?;
            changed = true;
            json!({"id": id})
        }
        Command::TabOpen { id } => {
            registry.open_tab(id).map_err(failed)?;
            changed = true;
            json!({"id": id})
        }
        Command::TabConfigure { id, config } => {
            registry.configure_plugin_tab(id, config).map_err(failed)?;
            changed = true;
            json!({"id": id})
        }
        Command::TabClose { id, confirmed } | Command::TabDelete { id, confirmed } => {
            if !confirmed
                && registry
                    .terminals
                    .get_mut(&id)
                    .is_some_and(|s| matches!(s.status(), Ok(Status::Running)))
            {
                return Err(RpcError::new(
                    "confirmation_required",
                    "Running terminal requires --confirm",
                ));
            }
            if matches!(command, Command::TabDelete { .. }) {
                registry.delete_tab(id, confirmed).map_err(failed)?;
            } else {
                registry.close_tab(id, confirmed).map_err(failed)?;
            }
            changed = true;
            json!({"id": id})
        }
        Command::ProjectConfig { id } => {
            let project = registry
                .project(id)
                .map_err(|_| RpcError::new("not_found", "Project ID is unavailable"))?;
            let directory = crate::control::paths::config_directory().map_err(failed)?;
            let yaml = crate::actions::read_project_yaml(&directory, id).map_err(failed)?;
            json!({"project": project, "yaml": yaml})
        }
        Command::ProjectConfigure { id, yaml } => {
            registry
                .project(id)
                .map_err(|_| RpcError::new("not_found", "Project ID is unavailable"))?;
            let directory = crate::control::paths::config_directory().map_err(failed)?;
            crate::actions::save_project_yaml(&directory, id, &yaml).map_err(failed)?;
            changed = true;
            json!({"id": id})
        }
        Command::ProjectList => json!({"projects":snapshot.projects}),
        Command::ProjectCreate { name, root } => {
            let next = registry.create_project(name, root).map_err(failed)?;
            changed = true;
            serde_json::to_value(next.projects.last().unwrap())
                .map_err(|e| failed(e.to_string()))?
        }
        Command::ProjectRename { id, name } => {
            let old = snapshot
                .projects
                .iter()
                .find(|g| g.id == id)
                .ok_or_else(|| RpcError::new("not_found", "Project ID is unavailable"))?;
            registry
                .update_project(id, name, old.root_directory.clone())
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(registry.project(id).map_err(failed)?)
                .map_err(|e| failed(e.to_string()))?
        }
        Command::ProjectDirectory { id, directory } => {
            let old = snapshot
                .projects
                .iter()
                .find(|g| g.id == id)
                .ok_or_else(|| RpcError::new("not_found", "Project ID is unavailable"))?;
            registry
                .update_project(id, old.name.clone(), directory)
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(registry.project(id).map_err(failed)?)
                .map_err(|e| failed(e.to_string()))?
        }
        Command::ProjectFold { id, collapsed } => {
            if !snapshot.projects.iter().any(|g| g.id == id) {
                return Err(RpcError::new("not_found", "Project ID is unavailable"));
            }
            registry.fold_project(id, collapsed).map_err(failed)?;
            changed = true;
            serde_json::to_value(registry.project(id).map_err(failed)?)
                .map_err(|e| failed(e.to_string()))?
        }
        Command::ProjectClose { id } => {
            if !snapshot.projects.iter().any(|g| g.id == id) {
                return Err(RpcError::new("not_found", "Project ID is unavailable"));
            }
            registry.close_project(id).map_err(failed)?;
            changed = true;
            json!({"closed":id})
        }
    };
    drop(registry);
    if changed {
        if let Err(error) = app.emit("session-changed", ()) {
            eprintln!("Session change committed; UI notification failed: {error}");
        }
    }
    Ok((Some(data), effect))
}
