//! The current desktop run's rolling log, separate from the clearable Log panel.
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use texturecraft_engine::LogEntry;

// Retain two segments to bound each run's log while preserving recent history
const MAX_LOG_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
/// Runs whose log folders stay in the logs folder, the new run's included, so the folder stays bounded
const KEPT_SESSION_FOLDERS: usize = 10;
const SESSION_FOLDER_PREFIX: &str = "session-";

pub(crate) struct SessionLogFile {
    pub path: PathBuf,
    file: File,
    bytes: u64,
    log_id: u64,
    cursor: usize,
    last: Option<LogEntry>,
}

/// Remove all but the `keep` newest session folders in `logs`. Their names start with a UTC time, so
/// name order is age order. Errors are ignored: a folder an older run still holds open stays for a later start.
fn prune(logs: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(logs) else { return };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.starts_with(SESSION_FOLDER_PREFIX)))
        .map(|entry| entry.path())
        .collect();
    folders.sort();
    let old = folders.len().saturating_sub(keep);
    for folder in &folders[..old] {
        let _ = std::fs::remove_dir_all(folder);
    }
}

impl SessionLogFile {
    /// Start this run's log in a new session folder under `logs`, after removing the oldest runs' folders.
    pub fn start(logs: &Path) -> io::Result<Self> {
        prune(logs, KEPT_SESSION_FOLDERS - 1);
        let folder = format!("{SESSION_FOLDER_PREFIX}{}-{}", chrono::Utc::now().format("%Y%m%dT%H%M%S%.6fZ"), std::process::id());
        Self::new(logs.join(folder).join("log.txt"))
    }

    pub fn new(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create_new(true).append(true).open(&path)?;
        Ok(Self { path, file, bytes: 0, log_id: 0, cursor: 0, last: None })
    }

    pub fn text(&mut self) -> io::Result<String> {
        self.file.flush()?;
        let previous = self.path.with_file_name("log.previous.txt");
        let mut text = match std::fs::read_to_string(previous) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        text.push_str(&std::fs::read_to_string(&self.path)?);
        Ok(text)
    }

    pub fn sync(&mut self, log_id: u64, entries: &[LogEntry]) -> io::Result<()> {
        let same_tail = self.log_id == log_id && self.cursor.checked_sub(1).and_then(|i| entries.get(i)) == self.last.as_ref();
        if same_tail && self.cursor == entries.len() {
            return Ok(());
        }

        let start = if same_tail { self.cursor } else { 0 };
        for entry in entries.get(start..).unwrap_or_default() {
            // Only reconcile timestamps after removal; appended entries can predate the last sync
            if !same_tail && self.log_id == log_id && self.last.as_ref().is_some_and(|last| entry.time <= last.time) {
                continue;
            }

            let text = crate::windows::log_text(std::slice::from_ref(entry));
            let line = text.split_once('\n').map(|(_, line)| line).unwrap_or_default();

            // Keep one previous segment when the active log rolls
            if self.bytes.saturating_add(line.len() as u64) > MAX_LOG_SEGMENT_BYTES {
                self.file.flush()?;
                std::fs::copy(&self.path, self.path.with_file_name("log.previous.txt"))?;
                self.file.set_len(0)?;
                self.bytes = 0;
            }
            self.file.write_all(line.as_bytes())?;
            self.bytes = self.bytes.saturating_add(line.len() as u64);
        }
        self.file.flush()?;
        self.log_id = log_id;
        self.cursor = entries.len();
        self.last = entries.last().cloned();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use texturecraft_engine::{LogLevel, SessionLog};

    #[test]
    fn log_file_keeps_messages_after_panel_clear_and_across_projects() {
        let dir = std::env::temp_dir().join(format!("texturecraft-session-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{}.txt", chrono::Utc::now().timestamp_nanos_opt().unwrap()));
        let mut file = SessionLogFile::new(path.clone()).unwrap();
        let mut log = SessionLog::new();
        log.push("first".into());
        file.sync(1, log.entries()).unwrap();
        file.sync(1, log.entries()).unwrap();
        log.clear();
        file.sync(1, log.entries()).unwrap();
        log.error("UI", "second");
        file.sync(1, log.entries()).unwrap();
        file.sync(2, &[LogEntry::new(LogLevel::Info, "UI", "new project")]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("first").count(), 1);
        assert!(text.contains("second") && text.contains("new project"));
        drop(file);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn starting_a_run_removes_only_the_oldest_session_folders() {
        let logs = std::env::temp_dir().join(format!("texturecraft-session-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&logs);
        let older: Vec<PathBuf> = (0..KEPT_SESSION_FOLDERS + 2).map(|i| logs.join(format!("{SESSION_FOLDER_PREFIX}20260101T0000{i:02}.000000Z-1"))).collect();
        for folder in &older {
            std::fs::create_dir_all(folder).unwrap();
        }
        let other = logs.join("kept-by-the-user");
        std::fs::create_dir_all(&other).unwrap();

        let file = SessionLogFile::start(&logs).unwrap();

        let survivors = older.iter().filter(|folder| folder.exists()).count();
        assert_eq!(survivors + 1, KEPT_SESSION_FOLDERS, "the new run and the newest earlier runs remain");
        assert!(!older[0].exists() && older.last().unwrap().exists(), "the oldest go first");
        assert!(other.exists() && file.path.exists(), "other folders and the new log stay");
        drop(file);
        std::fs::remove_dir_all(logs).unwrap();
    }
}
