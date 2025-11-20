use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct DiffSnapshot {
    pub timestamp: SystemTime,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub status: String,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<String>,
}

impl Hunk {
    pub fn format(&self) -> String {
        self.lines.join("")
    }
}
