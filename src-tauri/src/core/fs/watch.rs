//! Attachment-owned watch identity; the system owns native notification resources.
pub struct Watch {
    pub path: String,
    pub token: String,
    native: Box<dyn crate::system::FileWatch>,
}
impl Watch {
    pub fn start(
        files: &dyn crate::system::SystemFiles,
        path: String,
        token: String,
        changed: impl Fn() + Send + 'static,
    ) -> Result<Self, String> {
        let native = files.watch(path.clone(), Box::new(changed))?;
        Ok(Self {
            path,
            token,
            native,
        })
    }
    pub fn acknowledge(&self) {
        self.native.acknowledge();
    }
}
