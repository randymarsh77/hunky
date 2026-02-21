use super::*;
use notify::{event::{CreateKind, ModifyKind, RemoveKind}, EventKind};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FS_STABILIZATION_DELAY: Duration = Duration::from_millis(700);
const WATCHER_RETRY_ATTEMPTS: usize = 3;
const WATCHER_RECV_TIMEOUT: Duration = Duration::from_secs(3);

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

#[test]
fn ignores_gitignored_files() {
    let repo = TestRepo::new();
    repo.write_file(".gitignore", "hunky.log\n");
    repo.commit_all("add ignore rule");

    let event = Event::new(EventKind::Modify(ModifyKind::Any))
        .add_path(repo.path.join("hunky.log"));

    assert!(!should_process_event(&event, &repo.path));
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

    tokio::time::sleep(FS_STABILIZATION_DELAY).await;

    for attempt in 0..WATCHER_RETRY_ATTEMPTS {
        repo.write_file("tracked.txt", &format!("line 1\nline {}\n", attempt + 2));
        if let Ok(Some(snapshot)) = tokio::time::timeout(WATCHER_RECV_TIMEOUT, rx.recv()).await {
            assert!(!snapshot.files.is_empty());
            assert!(snapshot.files.iter().any(|file| file.path.ends_with("tracked.txt")));
            return;
        }
        tokio::time::sleep(FS_STABILIZATION_DELAY).await;
    }

    panic!("watcher did not emit a snapshot in time");
}
