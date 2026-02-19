use anyhow::Result;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::io::Write;
use std::path::Path;
use tokio::sync::mpsc;

use crate::diff::DiffSnapshot;
use crate::git::GitRepo;

// Debug logging helper
fn debug_log(msg: String) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("hunky-debug.log")
    {
        let _ = writeln!(file, "[{}] {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), msg);
    }
}

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    pub fn new(
        git_repo: GitRepo,
        snapshot_sender: mpsc::UnboundedSender<DiffSnapshot>,
    ) -> Result<Self> {
        let repo_path = git_repo.repo_path().to_path_buf();
        
        let (tx, rx) = std::sync::mpsc::channel();
        
        let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
        
        watcher.watch(repo_path.as_ref(), RecursiveMode::Recursive)?;
        
        // Spawn a task to handle file system events
        tokio::spawn(async move {
            let mut last_snapshot_time = std::time::Instant::now();
            let debounce_duration = std::time::Duration::from_millis(500);
            
            debug_log(format!("File watcher started for {:?}", repo_path));
            
            loop {
                match rx.recv() {
                    Ok(Ok(event)) => {
                        debug_log(format!("Received event: {:?}", event));
                        // Only process events for git-tracked files
                        if should_process_event(&event, &repo_path) {
                            debug_log("Processing event for snapshot".to_string());
                            // Debounce: only create a new snapshot if enough time has passed
                            let now = std::time::Instant::now();
                            if now.duration_since(last_snapshot_time) >= debounce_duration {
                                if let Ok(snapshot) = git_repo.get_diff_snapshot() {
                                    debug_log(format!("Created snapshot with {} files", snapshot.files.len()));
                                    // Only send if there are actual changes
                                    if !snapshot.files.is_empty() {
                                        let _ = snapshot_sender.send(snapshot);
                                        last_snapshot_time = now;
                                    } else {
                                        debug_log("Snapshot was empty, not sending".to_string());
                                    }
                                }
                            } else {
                                debug_log("Debouncing, too soon since last snapshot".to_string());
                            }
                        } else {
                            debug_log("Event filtered out (likely .git directory)".to_string());
                        }
                    }
                    Ok(Err(e)) => {
                        debug_log(format!("Watch error: {:?}", e));
                    }
                    Err(_) => break,
                }
            }
        });
        
        Ok(Self { _watcher: watcher })
    }
}

fn should_process_event(event: &Event, repo_path: &Path) -> bool {
    use notify::EventKind;
    
    // Filter out events we don't care about
    match event.kind {
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
            // Check if any of the paths are:
            // 1. Not in .git directory (working directory changes), OR
            // 2. The .git/index file specifically (staging changes)
            event.paths.iter().any(|path| {
                // Check if it's the git index file
                if path.ends_with(".git/index") {
                    return true;
                }
                
                // Check if it's a working directory file (not in .git)
                path.strip_prefix(repo_path)
                    .ok()
                    .and_then(|p| p.components().next())
                    .map(|c| c.as_os_str() != ".git")
                    .unwrap_or(false)
            })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::{event::{CreateKind, ModifyKind, RemoveKind}, EventKind};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn processes_working_tree_modifications() {
        let repo_path = PathBuf::from("/tmp/repo");
        let event = Event::new(EventKind::Modify(ModifyKind::Any))
            .add_path(repo_path.join("src/main.rs"));

        assert!(should_process_event(&event, &repo_path));
    }

    #[test]
    fn ignores_git_directory_changes_except_index() {
        let repo_path = PathBuf::from("/tmp/repo");
        let git_object_event = Event::new(EventKind::Create(CreateKind::Any))
            .add_path(repo_path.join(".git/objects/ab/cdef"));
        let index_event =
            Event::new(EventKind::Modify(ModifyKind::Any)).add_path(repo_path.join(".git/index"));

        assert!(!should_process_event(&git_object_event, &repo_path));
        assert!(should_process_event(&index_event, &repo_path));
    }

    #[test]
    fn ignores_non_create_modify_remove_events() {
        let repo_path = PathBuf::from("/tmp/repo");
        let event =
            Event::new(EventKind::Remove(RemoveKind::Any)).add_path(repo_path.join("README.md"));
        assert!(should_process_event(&event, &repo_path));

        let access_event = Event::new(EventKind::Any).add_path(repo_path.join("README.md"));
        assert!(!should_process_event(&access_event, &repo_path));
    }

    struct TestRepo {
        path: PathBuf,
    }

    impl TestRepo {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("failed to get system time")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hunky-watcher-tests-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&path).expect("failed to create temp directory");
            run_git(&path, &["init"]);
            run_git(&path, &["config", "user.name", "Test User"]);
            run_git(&path, &["config", "user.email", "test@example.com"]);
            Self { path }
        }

        fn write_file(&self, rel_path: &str, content: &str) {
            fs::write(self.path.join(rel_path), content).expect("failed to write file");
        }

        fn commit_all(&self, message: &str) {
            run_git(&self.path, &["add", "."]);
            run_git(&self.path, &["commit", "-m", message]);
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn run_git(repo_path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .output()
            .expect("failed to execute git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn watcher_emits_snapshot_for_tracked_file_changes() {
        let repo = TestRepo::new();
        repo.write_file("tracked.txt", "line 1\n");
        repo.commit_all("initial");

        let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _watcher = FileWatcher::new(git_repo, tx).expect("failed to start watcher");

        tokio::time::sleep(Duration::from_millis(700)).await;

        for attempt in 0..3 {
            repo.write_file("tracked.txt", &format!("line 1\nline {}\n", attempt + 2));
            if let Ok(Some(snapshot)) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
                assert!(!snapshot.files.is_empty());
                assert!(snapshot.files.iter().any(|file| file.path.ends_with("tracked.txt")));
                return;
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }

        panic!("watcher did not emit a snapshot in time");
    }
}
