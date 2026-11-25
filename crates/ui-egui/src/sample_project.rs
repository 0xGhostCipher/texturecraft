//! The sample project File > Open Sample Project offers: written once into the Samples folder.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use texturecraft_engine::core::PROJECT_EXT;
use texturecraft_engine::{SAMPLE_BAKE_RESOLUTION_PX, SAMPLE_PROJECT_NAME, SAMPLE_RESOLUTION_PX, sample_session};

use crate::TextureCraftApp;
use crate::file_dialogs::{Bound, FileDialog};
use crate::i18n::{tr, trf};

/// The folder under the data folder that holds the sample projects
pub(crate) const SAMPLES_FOLDER: &str = "Samples";

/// The error when the background thread ends without sending its result
const STOPPED_WITHOUT_ANSWER: &str = "the preparation stopped without an answer";

/// The background job that writes the sample project file
pub(crate) type SamplePreparation = Receiver<Result<PathBuf, String>>;

/// The sample project's file in `folder`
fn sample_file(folder: &Path) -> PathBuf {
    folder.join(format!("{SAMPLE_PROJECT_NAME}.{PROJECT_EXT}"))
}

/// Makes sure `folder` exists and holds the built-in sample project, and returns the file.
/// A file that is already there is kept as it is, so a sample the user edited and saved stays.
fn ensure_sample_project(folder: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|error| format!("{}: {error}", folder.display()))?;

    let file = sample_file(folder);
    if file.exists() {
        return Ok(file);
    }

    let session = sample_session(SAMPLE_RESOLUTION_PX, SAMPLE_BAKE_RESOLUTION_PX).map_err(|error| error.to_string())?;
    let bytes = session.to_project_bytes().map_err(|error| error.to_string())?;

    // Baking takes a while, so look again before writing in case the file appeared meanwhile
    if file.exists() {
        return Ok(file);
    }

    texturecraft_engine::formats::write_atomic(&file, &bytes).map_err(|error| format!("{}: {error}", file.display()))?;

    Ok(file)
}

impl TextureCraftApp {
    /// File > Open Sample Project: the file picker, in the Samples folder. A missing sample is
    /// written on a background thread first, and the picker opens when that is done.
    pub(crate) fn open_sample_project(&mut self, ctx: &egui::Context) {
        if self.sample_preparation.is_some() {
            return;
        }

        let folder = self.samples_folder.clone();
        if sample_file(&folder).exists() {
            self.ask_for_sample_project(Some(folder));
            return;
        }

        self.toast(tr("Preparing the sample project..."), false);
        let (sender, receiver) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // The receiver is gone only when the app closed
            let _ = sender.send(ensure_sample_project(&folder));
            ctx.request_repaint();
        });
        self.sample_preparation = Some(receiver);
    }

    /// Whether the sample project is being written
    pub fn sample_project_pending(&self) -> bool {
        self.sample_preparation.is_some()
    }

    /// Takes the finished sample preparation, if any, and opens the picker. Called every frame.
    pub(crate) fn poll_sample_project(&mut self) {
        let Some(receiver) = &self.sample_preparation else { return };

        match receiver.try_recv() {
            Ok(result) => self.finish_sample_project(result),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.finish_sample_project(Err(STOPPED_WITHOUT_ANSWER.to_string())),
        }
    }

    /// Waits for the sample preparation to end and opens the picker, so tests need no timing.
    pub fn wait_for_sample_project(&mut self) {
        let Some(receiver) = &self.sample_preparation else { return };

        let result = receiver.recv().unwrap_or_else(|_| Err(STOPPED_WITHOUT_ANSWER.to_string()));
        self.finish_sample_project(result);
    }

    fn finish_sample_project(&mut self, result: Result<PathBuf, String>) {
        self.sample_preparation = None;
        match result {
            Ok(_) => self.ask_for_sample_project(Some(self.samples_folder.clone())),
            Err(error) => {
                self.toast(trf("Could not prepare the sample project: {error}", &[("error", &error)]), true);
                self.ask_for_sample_project(None);
            }
        }
    }

    fn ask_for_sample_project(&mut self, folder: Option<PathBuf>) {
        let dialog = FileDialog::open().title(tr("Open Sample Project")).filter(tr("TextureCraft project"), &[PROJECT_EXT]);
        let dialog = match folder {
            Some(folder) => dialog.directory(folder),
            None => dialog,
        };
        self.ask_file(dialog, Bound::App, crate::windows::open_picked);
    }
}
