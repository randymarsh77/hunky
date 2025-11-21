use std::collections::HashSet;
use std::hash::{Hash, Hasher};
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
    pub seen: bool,
    pub staged: bool,
    pub id: HunkId,
}

impl Hunk {
    pub fn format(&self) -> String {
        self.lines.join("")
    }
    
    pub fn count_changes(&self) -> usize {
        let mut add_lines = 0;
        let mut remove_lines = 0;
        
        for line in &self.lines {
            if line.starts_with('+') && !line.starts_with("+++") {
                add_lines += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                remove_lines += 1;
            }
        }
        
        // Count pairs of add/remove as 1 change, plus any unpaired lines
        let pairs = add_lines.min(remove_lines);
        let unpaired = (add_lines + remove_lines) - (2 * pairs);
        pairs + unpaired
    }
    
    pub fn new(old_start: usize, new_start: usize, lines: Vec<String>, file_path: &PathBuf) -> Self {
        let id = HunkId::new(file_path, old_start, new_start, &lines);
        Self {
            old_start,
            new_start,
            lines,
            seen: false,
            staged: false,
            id,
        }
    }
}

/// Unique identifier for a hunk based on file path, line numbers, and content hash
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HunkId {
    pub file_path: PathBuf,
    pub old_start: usize,
    pub new_start: usize,
    pub content_hash: u64,
}

impl HunkId {
    pub fn new(file_path: &PathBuf, old_start: usize, new_start: usize, lines: &[String]) -> Self {
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        for line in lines {
            line.hash(&mut hasher);
        }
        let content_hash = hasher.finish();
        
        Self {
            file_path: file_path.clone(),
            old_start,
            new_start,
            content_hash,
        }
    }
}

/// Tracks which hunks have been seen by the user
#[derive(Debug, Clone)]
pub struct SeenTracker {
    seen_hunks: HashSet<HunkId>,
}

impl SeenTracker {
    pub fn new() -> Self {
        Self {
            seen_hunks: HashSet::new(),
        }
    }
    
    pub fn mark_seen(&mut self, hunk_id: &HunkId) {
        self.seen_hunks.insert(hunk_id.clone());
    }
    
    pub fn is_seen(&self, hunk_id: &HunkId) -> bool {
        self.seen_hunks.contains(hunk_id)
    }
    
    pub fn clear(&mut self) {
        self.seen_hunks.clear();
    }
    
    pub fn remove_file_hunks(&mut self, file_path: &PathBuf) {
        self.seen_hunks.retain(|hunk_id| &hunk_id.file_path != file_path);
    }
}

impl Default for SeenTracker {
    fn default() -> Self {
        Self::new()
    }
}
