//! A button on a toast that acts on what the toast says

use crate::TextureCraftApp;

/// What a toast's button does
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastAction {
    /// "Update": read every outdated resource file again (the "changed on disk" notice)
    #[cfg(not(target_arch = "wasm32"))]
    UpdateResources,
}

impl ToastAction {
    /// The button's text (English; translated where it is drawn)
    pub fn label(self) -> &'static str {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            ToastAction::UpdateResources => "Update",
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn run(self, app: &mut TextureCraftApp) {
        match self {
            ToastAction::UpdateResources => crate::resources_updater::update(app, None),
        }
    }

    /// The browser's toasts carry no action yet
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn run(self, _: &mut TextureCraftApp) {
        match self {}
    }
}
