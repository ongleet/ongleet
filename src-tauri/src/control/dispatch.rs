use crate::{
    control::protocol::{Command, RpcError},
    session::Status,
    workspace::{Snapshot, Workspace, WorkspaceRegistry},
    Lifecycle, Sessions,
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
fn workspace(snapshot: &Snapshot, id: u64) -> Result<Workspace, RpcError> {
    snapshot
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .cloned()
        .ok_or_else(|| RpcError::new("not_found", "Workspace ID is unavailable"))
}
fn summary(registry: &mut WorkspaceRegistry, workspace: &Workspace) -> Result<Value, RpcError> {
    let mut tabs = Vec::new();
    let mut running = 0;
    for tab in workspace.tabs.iter().filter(|tab| tab.is_open) {
        let status = if let Some(id) = registry.sessions.get(&tab.id).map(|session| session.id) {
            let status = registry.status(id).map_err(failed)?;
            if matches!(status, Status::Running) {
                running += 1;
            }
            serde_json::to_value(status).map_err(|e| failed(e.to_string()))?
        } else {
            json!({"state": if tab.resume_on_start { "starting" } else { "stopped" }})
        };
        tabs.push(json!({"id":tab.id,"name":tab.name,"status":status}));
    }
    Ok(
        json!({"id":workspace.id,"workgroupId":workspace.workgroup_id,"name":workspace.name,"baseDirectory":workspace.base_directory,"displayOrder":workspace.display_order,"selectedTabId":workspace.selected_tab_id,"runningSessions":running,"tabs":tabs}),
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
    let shared = &app.state::<Sessions>().0;
    let mut registry = shared.lock().map_err(|e| failed(e.to_string()))?;
    let snapshot = registry.snapshot();
    let mut changed = false;
    let mut effect = Effect::None;
    let data = match command {
        Command::Show => unreachable!(),
        Command::Status => {
            let mut running = 0;
            for session in registry.sessions.values_mut() {
                if matches!(session.status().map_err(failed)?, Status::Running) {
                    running += 1;
                }
            }
            json!({"pid":std::process::id(),"visible":visible,"workgroups":snapshot.workgroups.len(),"workspaces":snapshot.workspaces.len(),"runningSessions":running})
        }
        Command::Quit { confirmed } => {
            if !confirmed {
                for session in registry.sessions.values_mut() {
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
        Command::WorkspaceList => {
            let mut list = Vec::new();
            for item in &snapshot.workspaces {
                list.push(summary(&mut registry, item)?);
            }
            json!({"workspaces":list,"selectedId":snapshot.selected_id})
        }
        Command::WorkspaceCreate {
            workgroup_id,
            name,
            directory,
        } => {
            if !snapshot.workgroups.iter().any(|g| g.id == workgroup_id) {
                return Err(RpcError::new("not_found", "Workgroup ID is unavailable"));
            }
            let directory_config = app
                .path()
                .app_config_dir()
                .map_err(|e| failed(e.to_string()))?;
            let next = registry
                .create_from_first_action(Some(workgroup_id), name, directory, &directory_config)
                .map_err(failed)?;
            changed = true;
            summary(&mut registry, &workspace(&next, next.selected_id.unwrap())?)?
        }
        Command::WorkspaceRename { id, name } => {
            let old = workspace(&snapshot, id)?;
            let next = registry
                .update(id, name, old.base_directory)
                .map_err(failed)?;
            changed = true;
            summary(&mut registry, &workspace(&next, id)?)?
        }
        Command::WorkspaceDirectory { id, directory } => {
            let old = workspace(&snapshot, id)?;
            let next = registry.update(id, old.name, directory).map_err(failed)?;
            changed = true;
            summary(&mut registry, &workspace(&next, id)?)?
        }
        Command::WorkspaceMove { id, workgroup_id } => {
            let old = workspace(&snapshot, id)?;
            if !snapshot.workgroups.iter().any(|g| g.id == workgroup_id) {
                return Err(RpcError::new("not_found", "Workgroup ID is unavailable"));
            }
            let next = registry
                .update_in_workgroup(id, workgroup_id, old.name, old.base_directory)
                .map_err(failed)?;
            changed = true;
            summary(&mut registry, &workspace(&next, id)?)?
        }
        Command::WorkspaceReorder { id, position } => {
            let old = workspace(&snapshot, id)?;
            let target = snapshot
                .workspaces
                .iter()
                .filter(|w| w.workgroup_id == old.workgroup_id)
                .nth(position)
                .ok_or_else(|| {
                    RpcError::new(
                        "invalid_argument",
                        "Position is outside this workgroup's workspace list",
                    )
                })?
                .display_order;
            let next = registry.reorder(id, target).map_err(failed)?;
            changed = true;
            summary(&mut registry, &workspace(&next, id)?)?
        }
        Command::WorkspaceSelect { id } => {
            workspace(&snapshot, id)?;
            let next = registry.select(id).map_err(failed)?;
            changed = true;
            summary(&mut registry, &workspace(&next, id)?)?
        }
        Command::WorkspaceClose { id, confirmed } => {
            let old = workspace(&snapshot, id)?;
            if summary(&mut registry, &old)?["runningSessions"]
                .as_u64()
                .unwrap_or(0)
                > 0
                && !confirmed
            {
                return Err(RpcError::new(
                    "confirmation_required",
                    "Running workspace processes require --confirm to close",
                ));
            }
            // The registry independently preflights every session before signals.
            registry.close(id, confirmed).map_err(failed)?;
            changed = true;
            json!({"closed":id})
        }
        Command::WorkgroupList => json!({"workgroups":snapshot.workgroups}),
        Command::WorkgroupCreate { name, root } => {
            let next = registry.create_workgroup(name, root).map_err(failed)?;
            changed = true;
            serde_json::to_value(next.workgroups.last().unwrap())
                .map_err(|e| failed(e.to_string()))?
        }
        Command::WorkgroupRename { id, name } => {
            let old = snapshot
                .workgroups
                .iter()
                .find(|g| g.id == id)
                .ok_or_else(|| RpcError::new("not_found", "Workgroup ID is unavailable"))?;
            registry
                .update_workgroup(id, name, old.root_directory.clone())
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(registry.workgroup(id).map_err(failed)?)
                .map_err(|e| failed(e.to_string()))?
        }
        Command::WorkgroupDirectory { id, directory } => {
            let old = snapshot
                .workgroups
                .iter()
                .find(|g| g.id == id)
                .ok_or_else(|| RpcError::new("not_found", "Workgroup ID is unavailable"))?;
            registry
                .update_workgroup(id, old.name.clone(), directory)
                .map_err(failed)?;
            changed = true;
            serde_json::to_value(registry.workgroup(id).map_err(failed)?)
                .map_err(|e| failed(e.to_string()))?
        }
        Command::WorkgroupFold { id, collapsed } => {
            if !snapshot.workgroups.iter().any(|g| g.id == id) {
                return Err(RpcError::new("not_found", "Workgroup ID is unavailable"));
            }
            registry.fold_workgroup(id, collapsed).map_err(failed)?;
            changed = true;
            serde_json::to_value(registry.workgroup(id).map_err(failed)?)
                .map_err(|e| failed(e.to_string()))?
        }
        Command::WorkgroupClose { id } => {
            if !snapshot.workgroups.iter().any(|g| g.id == id) {
                return Err(RpcError::new("not_found", "Workgroup ID is unavailable"));
            }
            registry.close_workgroup(id).map_err(failed)?;
            changed = true;
            json!({"closed":id})
        }
    };
    drop(registry);
    if changed {
        if let Err(error) = app.emit("workspace-changed", ()) {
            eprintln!("Workspace change committed; UI notification failed: {error}");
        }
    }
    Ok((Some(data), effect))
}
