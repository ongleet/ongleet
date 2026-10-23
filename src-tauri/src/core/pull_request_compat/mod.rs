//! Compatibility adapter for legacy CLI pullRequest fields.
use serde::{Deserialize, Serialize};
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Association {
    pub repository: String,
    pub number: u64,
}
impl Association {
    pub fn parse(target: &str, number: Option<u64>) -> Result<Self, String> {
        let (repository, number) = if let Some(path) = target.strip_prefix("https://github.com/") {
            let parts: Vec<_> = path.trim_end_matches('/').split('/').collect();
            if parts.len() != 4 || parts[2] != "pull" {
                return Err("Expected a GitHub pull request URL".into());
            }
            (
                format!("{}/{}", parts[0], parts[1]),
                parts[3].parse::<u64>().map_err(|_| "Invalid PR number")?,
            )
        } else {
            (target.to_owned(), number.ok_or("PR number is required")?)
        };
        let association = Self { repository, number };
        association.validate()?;
        Ok(association)
    }
    pub fn validate(&self) -> Result<(), String> {
        let parts: Vec<_> = self.repository.split('/').collect();
        if self.repository.len() > 256
            || parts.len() != 2
            || parts.iter().any(|s| {
                s.is_empty()
                    || s.starts_with(['-', '.'])
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
            || self.number == 0
        {
            return Err("Expected owner/repository and a positive PR number".into());
        }
        Ok(())
    }
}
