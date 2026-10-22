use super::{valid_name, validate_directory, SessionRegistry, Snapshot};
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub root_directory: String,
    pub display_order: usize,
    pub collapsed: bool,
}

impl SessionRegistry {
    pub fn project(&self, id: u64) -> Result<&Project, String> {
        self.projects
            .iter()
            .find(|g| g.id == id)
            .ok_or("Project is unavailable".into())
    }

    pub fn session_project(&self, id: u64) -> Result<u64, String> {
        self.sessions
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.project_id)
            .ok_or("Session is unavailable".into())
    }

    pub fn create_project(&mut self, name: String, root: String) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        validate_directory(&root)?;
        self.next_project_id += 1;
        self.projects.push(Project {
            id: self.next_project_id,
            name,
            root_directory: root,
            display_order: self.projects.len(),
            collapsed: false,
        });
        self.persist()
    }

    pub fn update_project(
        &mut self,
        id: u64,
        name: String,
        root: String,
    ) -> Result<Snapshot, String> {
        let name = valid_name(name)?;
        if self.project(id)?.root_directory != root {
            validate_directory(&root)?;
        }
        let group = self.projects.iter_mut().find(|g| g.id == id).unwrap();
        group.name = name;
        group.root_directory = root;
        self.persist()
    }

    pub fn fold_project(&mut self, id: u64, collapsed: bool) -> Result<Snapshot, String> {
        self.project(id)?;
        self.projects
            .iter_mut()
            .find(|g| g.id == id)
            .unwrap()
            .collapsed = collapsed;
        self.persist()
    }

    pub fn reorder_project(&mut self, id: u64, position: usize) -> Result<Snapshot, String> {
        if position >= self.projects.len() {
            return Err("Invalid project position".into());
        }
        let index = self
            .projects
            .iter()
            .position(|project| project.id == id)
            .ok_or("Project is unavailable")?;
        let project = self.projects.remove(index);
        self.projects.insert(position, project);
        for (order, project) in self.projects.iter_mut().enumerate() {
            project.display_order = order;
        }
        self.persist()
    }

    pub fn close_project(&mut self, id: u64) -> Result<Snapshot, String> {
        self.project(id)?;
        if self.sessions.iter().any(|w| w.project_id == id) {
            return Err("Move or close this project's sessions before deleting it".into());
        }
        self.projects.retain(|g| g.id != id);
        for (order, group) in self.projects.iter_mut().enumerate() {
            group.display_order = order;
        }
        self.persist()
    }
}
