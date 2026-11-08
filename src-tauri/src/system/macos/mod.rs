//! Darwin mechanisms; all application policy remains in the shared services.
use std::sync::Mutex;
pub(super) struct Macos {
    primary: Mutex<String>,
    image: paths::Image,
    diagnostics: std::sync::Arc<
        Mutex<Option<std::sync::Arc<dyn Fn(super::NativeDiagnostic) + Send + Sync>>>,
    >,
}
impl Macos {
    pub fn new() -> Self {
        Self {
            primary: Mutex::new(String::new()),
            image: paths::Image::capture(),
            diagnostics: std::sync::Arc::new(Mutex::new(None)),
        }
    }
}
pub(super) mod containment;
mod desktop;
mod files;
mod paths;
mod process;
mod secrets;
mod terminal;
mod transport;
mod watch;

mod events;

#[cfg(test)]
mod tests;

mod clipboard;
mod input;
