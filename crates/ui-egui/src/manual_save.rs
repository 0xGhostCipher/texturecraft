//! Manual desktop saves share the atomic archive writer with autosave and headless saves.

use std::sync::{Arc, mpsc};
use std::time::Duration;

use texturecraft_engine::{Result, file_cmds::SaveOutput, formats::project_file::SaveProgress};

use crate::{
    TextureCraftApp,
    i18n::{tr, trf},
    state::Unsaved,
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

        let mut quitting = false;
        match result {
            Ok(output) => {
                output.apply(&mut self.session);
                if job.remember {
                    crate::windows::remember(self, &output.path);
                }
                let clean = same_project && !self.session.modified && !self.session.stroke_active();
                quitting = job.quit_after_save && clean;
                self.prompt_saved = continue_prompt && clean && !quitting;
                self.toast(trf("Saved {path}", &[("path", &output.path)]), false);
                if quitting {
                    self.ui.windows.unsaved = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(error) => {
                let message = trf("Could not save project: {error}", &[("error", &error)]);
                self.session.log.warn("Project", message.clone());
                self.toast(message, true);
            }
        }

        // A close that the save could not finish asks about the changes it left
        if job.quit_after_save && !quitting {
            crate::windows::ask_to_save(self, Unsaved::Quit);
        }
        if let Some(toast) = self.toasts.last_mut() {
            toast.announce = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{ViewportCommand, ViewportEvent, ViewportId};
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT as _, Queryable as _};
    use texturecraft_engine::file_cmds::SaveTask;

    use super::*;

    fn harness() -> Harness<'static, TextureCraftApp> {
        let mut session = texturecraft_engine::sample_session(256, 64).unwrap();
        session.modified = true;
        let mut h = Harness::builder().with_size(egui::vec2(1600.0, 960.0)).build_eframe(|cc| TextureCraftApp::new(cc, session));

        // Match English labels independently of the system locale
        h.state_mut().ui.prefs.language = "en".into();
        h.run_steps(2);
        h
    }

    fn temp_file(test: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tc-manual-save-{test}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("project.tcraft")
    }

    /// A save the test finishes by hand, so the close lands while it is still running
    fn start_held_save(h: &mut Harness<'_, TextureCraftApp>, path: &std::path::Path, adopt: bool) -> (SaveTask, mpsc::Sender<Result<SaveOutput>>) {
        let app = h.state_mut();
        let task = app.session.save_task(&path.display().to_string(), adopt);
        let (sender, output) = mpsc::channel();
        app.save_job =
            Some(SaveJob { progress: Arc::default(), output, project_id: app.session.project_id, for_prompt: false, remember: false, quit_after_save: false });
        (task, sender)
    }

    fn request_close(h: &mut Harness<'_, TextureCraftApp>) {
        h.input_mut().viewports.entry(ViewportId::ROOT).or_default().events.push(ViewportEvent::Close);
        h.step();
        h.input_mut().viewports.entry(ViewportId::ROOT).or_default().events.clear();
        h.run_steps(2);
    }

    /// Hands the worker's result over and reports whether the app asked to close in the next frames
    fn finish(h: &mut Harness<'_, TextureCraftApp>, task: SaveTask, sender: &mpsc::Sender<Result<SaveOutput>>) -> bool {
        sender.send(task.run(None)).unwrap();
        (0..3).any(|_| {
            h.step();
            h.output().viewport_output.get(&ViewportId::ROOT).is_some_and(|v| v.commands.contains(&ViewportCommand::Close))
        })
    }

    fn prompt_is_usable(h: &Harness<'_, TextureCraftApp>) -> bool {
        ["Save", "Don't save", "Cancel"].iter().all(|label| !h.get_by_label(label).accesskit_node().is_disabled())
    }

    #[test]
    fn closing_during_a_regular_save_quits_once_it_has_saved() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = temp_file("close-then-saved");
        let mut h = harness();
        h.state_mut().session.path = Some(file.display().to_string());
        let (task, sender) = start_held_save(&mut h, &file, true);

        request_close(&mut h);
        assert_eq!(h.state().ui.windows.unsaved, Some(Unsaved::Quit), "the changes are not written yet");
        assert!(h.get_by_label("Save").accesskit_node().is_disabled(), "the prompt waits for the running save");
        let closed = finish(&mut h, task, &sender);

        assert!(closed, "the close goes on after the save");
        assert!(file.exists());
        assert!(!h.state().session.modified);
        assert!(h.state().ui.windows.unsaved.is_none(), "no question is left once the changes are saved");
    }

    #[test]
    fn closing_during_a_failed_save_keeps_a_usable_prompt() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = temp_file("close-then-failed").with_file_name("missing").join("project.tcraft");
        let mut h = harness();
        let (task, sender) = start_held_save(&mut h, &file, true);

        request_close(&mut h);
        let closed = finish(&mut h, task, &sender);

        assert!(!closed);
        assert!(h.state().session.modified);
        assert_eq!(h.state().ui.windows.unsaved, Some(Unsaved::Quit));
        assert!(prompt_is_usable(&h), "the prompt answers again after the failed write");
    }

    #[test]
    fn closing_during_a_save_a_copy_keeps_the_prompt() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = temp_file("close-then-copied");
        let mut h = harness();
        let (task, sender) = start_held_save(&mut h, &file, false);

        request_close(&mut h);
        let closed = finish(&mut h, task, &sender);

        assert!(!closed, "a copy leaves the project's own changes unsaved");
        assert!(file.exists());
        assert!(h.state().session.modified);
        assert_eq!(h.state().ui.windows.unsaved, Some(Unsaved::Quit));
        assert!(prompt_is_usable(&h));
    }

    #[test]
    fn closing_during_a_save_keeps_the_prompt_for_later_edits() {
        let _language = crate::i18n::TEST_LANGUAGE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let file = temp_file("close-then-edited");
        let mut h = harness();
        h.state_mut().session.path = Some(file.display().to_string());
        let (task, sender) = start_held_save(&mut h, &file, true);
        let rename = h.state_mut().run("textureSet.rename", serde_json::json!({"ts": 0, "name": "Edited while saving"}));
        assert!(rename.is_some());

        request_close(&mut h);
        let closed = finish(&mut h, task, &sender);

        assert!(!closed, "the edit made after the snapshot is not in the file");
        assert!(h.state().session.modified);
        assert_eq!(h.state().ui.windows.unsaved, Some(Unsaved::Quit));
        assert!(prompt_is_usable(&h));
    }
}
