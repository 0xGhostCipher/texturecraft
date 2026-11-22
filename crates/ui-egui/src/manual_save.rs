//! Manual desktop saves share the atomic archive writer with autosave and headless saves.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use texturecraft_engine::{Result, file_cmds::SaveOutput, formats::project_file::SaveProgress};

use crate::{
    TextureCraftApp,
    i18n::{tr, trf},
};

pub(crate) struct SaveJob {
    progress: Arc<SaveProgress>,
    output: mpsc::Receiver<Result<SaveOutput>>,
    project_id: u64,
    for_prompt: bool,
    remember: bool,
    pub(crate) quit_after_save: bool,
}

pub(crate) fn is_save(id: &str) -> bool {
    matches!(id, "file.save" | "file.saveAs" | "file.saveCopy")
}

impl TextureCraftApp {
    pub(crate) fn start_save(&mut self, path: &str, adopt: bool, for_prompt: bool, remember: bool) -> bool {
        if self.save_job.is_some() {
            self.toast(tr("A project save is already running"), true);
            return false;
        }
        if let Err(error) = self.session.end_stroke() {
            self.toast(error.to_string(), true);
            return false;
        }

        let task = self.session.save_task(path, adopt);
        let progress = Arc::new(SaveProgress::default());
        let worker_progress = progress.clone();
        let output = crate::spawn_worker("texturecraft-save", move || task.run(Some(&worker_progress)));
        self.save_job = Some(SaveJob { progress, output, project_id: self.session.project_id, for_prompt, remember, quit_after_save: false });
        true
    }

    pub(crate) fn save_progress(&self) -> Option<f32> {
        self.save_job.as_ref().map(|job| job.progress.fraction())
    }

    pub(crate) fn poll_save(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.save_job else { return };
        let result = match job.output.try_recv() {
            Ok(result) => result.map_err(|error| error.to_string()),
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(80));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(tr("The project save worker stopped").into()),
        };
        let Some(job) = self.save_job.take() else { return };
        let same_project = job.project_id == self.session.project_id;
        let continue_prompt = job.for_prompt && self.saving_for_prompt;
        if job.for_prompt {
            self.saving_for_prompt = false;
        }

        match result {
            Ok(output) => {
                output.apply(&mut self.session);
                if job.remember {
                    crate::windows::remember(self, &output.path);
                }
                self.prompt_saved = continue_prompt && same_project && !self.session.modified && !self.session.stroke_active();
                self.toast(trf("Saved {path}", &[("path", &output.path)]), false);
                if job.quit_after_save && same_project {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(error) => {
                let message = trf("Could not save project: {error}", &[("error", &error)]);
                self.session.log.warn("Project", message.clone());
                self.toast(message, true);
            }
        }
        if let Some(toast) = self.toasts.last_mut() {
            toast.announce = true;
        }
    }
}
