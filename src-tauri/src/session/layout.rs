//! Session-owned split metadata; plugin attachments and PTYs stay tab-owned.
use super::{Namespace, Session, SessionRegistry, Snapshot};
use serde::{Deserialize, Serialize};
const KEY: &str = "ongleet.layout";
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub direction: Option<String>,
    pub secondary: Vec<u64>,
    pub selected: Option<u64>,
    pub ratio: Option<f64>,
}
impl Session {
    pub(super) fn activate_layout_tab(&mut self, id: u64, newly_opened: bool) {
        let mut layout = self.layout();
        let was_secondary = self
            .selected_tab_id
            .is_some_and(|old| layout.secondary.contains(&old));
        if newly_opened && was_secondary && !layout.secondary.contains(&id) {
            layout.secondary.push(id);
        }
        if layout.secondary.contains(&id) != was_secondary {
            layout.selected = self.selected_tab_id;
        }
        self.store_layout(layout);
        self.selected_tab_id = Some(id);
    }
    pub fn layout(&self) -> Layout {
        self.plugin_data
            .get(KEY)
            .and_then(|n| serde_json::from_value(n.value.clone()).ok())
            .unwrap_or_default()
    }
    pub(super) fn store_layout(&mut self, layout: Layout) {
        self.plugin_data.insert(
            KEY.into(),
            Namespace {
                version: 1,
                revision: 0,
                value: serde_json::to_value(layout).unwrap(),
            },
        );
    }
    pub fn normalize_layout(&mut self) {
        let mut l = self.layout();
        let open: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.is_open)
            .map(|t| t.id)
            .collect();
        l.secondary.retain(|id| open.contains(id));
        if l.direction.is_none()
            || l.secondary.is_empty()
            || open.iter().all(|id| l.secondary.contains(id))
        {
            l = Layout::default();
        } else if !l.selected.is_some_and(|id| {
            open.contains(&id)
                && l.secondary.contains(&id)
                    != self
                        .selected_tab_id
                        .is_some_and(|id| l.secondary.contains(&id))
        }) {
            l.selected = open.iter().copied().find(|id| {
                l.secondary.contains(id)
                    != self
                        .selected_tab_id
                        .is_some_and(|id| l.secondary.contains(&id))
            });
        }
        self.store_layout(l);
    }
}
impl SessionRegistry {
    pub fn set_tab_layout(
        &mut self,
        id: u64,
        direction: Option<String>,
        secondary: Vec<u64>,
        selected: Option<u64>,
        ratio: f64,
    ) -> Result<Snapshot, String> {
        if !ratio.is_finite() || !(0.2..=0.8).contains(&ratio) {
            return Err("Invalid split ratio".into());
        }
        if direction
            .as_deref()
            .is_some_and(|d| d != "right" && d != "below")
        {
            return Err("Invalid split direction".into());
        }
        let session = self
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("Session not found")?;
        if secondary
            .iter()
            .any(|id| !session.tabs.iter().any(|t| t.id == *id && t.is_open))
        {
            return Err("Invalid group tab".into());
        }
        if secondary
            .iter()
            .enumerate()
            .any(|(i, id)| secondary[..i].contains(id))
        {
            return Err("Duplicate group tab".into());
        }
        if direction.is_none() {
            let old = session.layout();
            session
                .tabs
                .sort_by_key(|t| (old.secondary.contains(&t.id), t.display_order));
            for (order, tab) in session.tabs.iter_mut().enumerate() {
                tab.display_order = order;
            }
        }
        session.store_layout(Layout {
            direction,
            secondary,
            selected,
            ratio: Some(ratio),
        });
        self.persist()
    }
}
