//! The current desktop run's rolling log, separate from the clearable Log panel.
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
};
use texturecraft_engine::LogEntry;

pub(crate) struct SessionLogFile {
    pub path: PathBuf,
    file: File,
    bytes: u64,
    log_id: u64,
    cursor: usize,
    last: Option<LogEntry>,
}

impl SessionLogFile {
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
            // Clearing lines in the panel must not append the remaining old lines twice.
            if self.log_id == log_id && self.last.as_ref().is_some_and(|last| entry.time <= last.time) {
                continue;
            }
            let text = crate::windows::log_text(std::slice::from_ref(entry));
            let line = text.split_once('\n').map(|(_, line)| line).unwrap_or_default();
            // Bound each run's disk use. Keep one previous segment when the active log rolls.
            if self.bytes.saturating_add(line.len() as u64) > 8 * 1024 * 1024 {
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
        if let Some(last) = entries.last() {
            self.last = Some(last.clone());
        }
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
}
