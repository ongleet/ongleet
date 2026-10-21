use super::{valid_name, validate_directory, Snapshot, WorkspaceRegistry};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workgroup {
    pub id: u64,
    pub name: String,
    pub root_directory: String,
    pub display_order: usize,
    pub collapsed: bool,
}

impl WorkspaceRegistry {
    pub fn workgroup(&self, id: u64) -> Result<&Workgroup, String> {
        self.workgroups
            .iter()
            .find(|g| g.id == id)
            .ok_or("Workgroup is unavailable".into())
    }

    pub fn workspace_workgroup(&self, id: u64) -> Result<u64, String> {
        self.workspaces
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.workgroup_id)
            .ok_or("Workspace is unavailable".into())
    }

    pub fn create_workgroup(&mut self, name: String, root: String) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        validate_directory(&root)?;
        self.next_workgroup_id += 1;
        self.workgroups.push(Workgroup {
            id: self.next_workgroup_id,
            name,
            root_directory: root,
            display_order: self.workgroups.len(),
            collapsed: false,
        });
        self.persist()
    }

    pub fn update_workgroup(
        &mut self,
        id: u64,
        name: String,
        root: String,
    ) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        if self.workgroup(id)?.root_directory != root {
            validate_directory(&root)?;
        }
        let group = self.workgroups.iter_mut().find(|g| g.id == id).unwrap();
        group.name = name;
        group.root_directory = root;
        self.persist()
    }

    pub fn fold_workgroup(&mut self, id: u64, collapsed: bool) -> Result<Snapshot, String> {
        self.workgroup(id)?;
        self.workgroups
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap()
            .collapsed = collapsed;
        self.persist()
    }

    pub fn close_workgroup(&mut self, id: u64) -> Result<Snapshot, String> {
        self.workgroup(id)?;
        if self.workspaces.iter().any(|w| w.workgroup_id == id) {
            return Err("Move or close this workgroup's workspaces before deleting it".into());
        }
        self.workgroups.retain(|g| g.id != id);
        for (order, group) in self.workgroups.iter_mut().enumerate() {
            group.display_order = order;
        }
        self.persist()
    }
}
