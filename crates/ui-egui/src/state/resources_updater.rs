use texturecraft_engine::{ResourceState, ResourceStatus};

/// The Resources Updater window: the project's image resources as last checked, and the filters
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ResourcesUpdater {
    /// The resources and how they compare with their files, outdated first (checked on opening,
    /// Refresh, every change and when the app gets the focus back)
    pub rows: Vec<ResourceState>,
    /// The status the list is narrowed to (None: all)
    pub status_filter: Option<ResourceStatus>,
    /// Shows only resources whose name contains this, ignoring case
    pub name_filter: String,
    /// Top or Bottom asked the list to scroll to that end in the next frame
    pub jump: Option<egui::Align>,
    /// The project that was open when this window opened
    pub project: u64,
    /// The session revision the rows were checked at
    pub(crate) checked_revision: u64,
}
