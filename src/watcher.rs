use anyhow::Result;
use git2::Repository;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::Path;
use tokio::sync::mpsc;

use crate::diff::DiffSnapshot;
use crate::git::GitRepo;

// Debug logging helper
fn debug_log(msg: String) {
    crate::logger::debug(msg);
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

        std::thread::spawn(move || {
            use std::sync::mpsc::RecvTimeoutError;

            let mut deadline: Option<std::time::Instant> = None;
            let debounce_duration = std::time::Duration::from_millis(500);

            debug_log(format!("File watcher started for {:?}", repo_path));

            loop {
                let event = match deadline {
                    Some(deadline) => rx.recv_timeout(
                        deadline.saturating_duration_since(std::time::Instant::now()),
                    ),
                    None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };
                match event {
                    Ok(Ok(event)) => {
                        // Only process events for git-tracked files
                        if should_process_event(&event, &repo_path) {
                            debug_log(format!("Received event: {:?}", event));
                            debug_log("Processing event for snapshot".to_string());
                            deadline = Some(std::time::Instant::now() + debounce_duration);
                        } else if crate::logger::filtered_events_enabled() {
                            crate::logger::trace(format!("Filtered event: {:?}", event));
                        }
                    }
                    Ok(Err(e)) => {
                        crate::logger::warn(format!("Watch error: {:?}", e));
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        deadline = None;
                        match git_repo.get_diff_snapshot() {
                            Ok(snapshot) => {
                                debug_log(format!(
                                    "Created snapshot with {} files",
                                    snapshot.files.len()
                                ));
                                if snapshot_sender.send(snapshot).is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                crate::logger::warn(format!("Failed to refresh Git diff: {error}"));
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
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
                let rel_path = match path.strip_prefix(repo_path) {
                    Ok(p) => p,
                    Err(_) => return false,
                };

                if rel_path
                    .components()
                    .next()
                    .map(|c| c.as_os_str() == ".git")
                    .unwrap_or(false)
                {
                    return false;
                }

                // Ignore files excluded by gitignore/excludes.
                !is_git_ignored(repo_path, rel_path)
            })
        }
        _ => false,
    }
}

fn is_git_ignored(repo_path: &Path, rel_path: &Path) -> bool {
    match Repository::open(repo_path) {
        Ok(repo) => repo.status_should_ignore(rel_path).unwrap_or(false),
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "../tests/watcher.rs"]
mod tests;
