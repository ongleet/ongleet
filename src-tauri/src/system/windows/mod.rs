//! Windows mechanisms. Jobs and native handles never cross the service boundary.
use std::sync::{Arc, Mutex};
mod desktop;
mod files;
mod native;
mod paths;
mod process;
mod secrets;
mod terminal;
#[cfg(test)]
mod tests;
mod transport;
mod watch;
pub(super) struct Windows {
    primary: Mutex<String>,
    #[cfg(test)]
    secret_path: Option<std::path::PathBuf>,
    image: paths::Image,
    diagnostics: Arc<Mutex<Option<Arc<dyn Fn(super::NativeDiagnostic) + Send + Sync>>>>,
}
impl Windows {
    pub fn new() -> Self {
        Self {
            primary: Mutex::new(String::new()),
            #[cfg(test)]
            secret_path: None,
            image: paths::Image::capture(),
            diagnostics: Arc::new(Mutex::new(None)),
        }
    }
}

pub(super) fn run_helper_if_requested() -> bool {
    process::run_helper_if_requested()
}
