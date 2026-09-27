use super::*;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("failed to get system time")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("hunky-git-tests-{}-{}", std::process::id(), unique));

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

fn run_git(repo_path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .expect("failed to execute git");

    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn new_discovers_repo_from_nested_path_and_reports_repo_path() {
    let repo = TestRepo::new();
    fs::create_dir_all(repo.path.join("nested/dir")).expect("failed to create nested path");

    let git_repo = GitRepo::new(repo.path.join("nested/dir")).expect("failed to discover repo");
    assert_eq!(git_repo.repo_path(), repo.path.as_path());
}

#[test]
fn new_returns_error_for_non_repo_path() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("failed to get system time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "hunky-non-repo-tests-{}-{}",
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&path).expect("failed to create temp directory");

    let result = GitRepo::new(&path);
    assert!(result.is_err(), "expected non-repo path to fail");

    let _ = fs::remove_dir_all(path);
}

#[test]
fn commit_with_editor_returns_non_success_when_nothing_to_commit() {
    let repo = TestRepo::new();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");

    let status = git_repo
        .commit_with_editor()
        .expect("failed to run git commit");
    assert!(!status.success());
}

#[test]
fn stage_and_unstage_file_updates_index() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline 2\nline 3\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let file_path = Path::new("example.txt");

    git_repo
        .stage_file(file_path)
        .expect("failed to stage file");
    let staged = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("example.txt"));

    git_repo
        .unstage_file(file_path)
        .expect("failed to unstage file");
    let staged_after = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged_after.trim().is_empty());
}

#[test]
fn stage_and_unstage_added_and_deleted_files_updates_index() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "tracked\n");
    repo.commit_all("initial");

    repo.write_file("added.txt", "new file\n");
    fs::remove_file(repo.path.join("tracked.txt")).expect("failed to remove tracked file");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");

    git_repo
        .stage_file(Path::new("added.txt"))
        .expect("failed to stage added file");
    git_repo
        .stage_file(Path::new("tracked.txt"))
        .expect("failed to stage deleted file");

    let staged = run_git(&repo.path, &["diff", "--cached", "--name-status"]);
    assert!(staged.contains("A\tadded.txt"));
    assert!(staged.contains("D\ttracked.txt"));

    git_repo
        .unstage_file(Path::new("added.txt"))
        .expect("failed to unstage added file");
    git_repo
        .unstage_file(Path::new("tracked.txt"))
        .expect("failed to unstage deleted file");

    let staged_after = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged_after.trim().is_empty());
}

#[test]
fn stage_and_unstage_hunk_updates_index() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\nline 3\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline two updated\nline 3\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");
    let file_path = Path::new("example.txt");

    git_repo
        .stage_hunk(hunk, file_path)
        .expect("failed to stage hunk");
    let staged = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("example.txt"));

    let staged_lines = git_repo
        .detect_staged_lines(hunk, file_path)
        .expect("failed to detect staged lines");
    assert!(!staged_lines.is_empty());

    git_repo
        .unstage_hunk(hunk, file_path)
        .expect("failed to unstage hunk");
    let staged_after = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged_after.trim().is_empty());
}

#[test]
fn stage_and_unstage_single_line_tracks_index_changes() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "one\ntwo\nthree\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "one\ntwo-updated\nthree\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");
    let line_index = hunk
        .lines
        .iter()
        .position(|line| line.starts_with('+') && !line.starts_with("+++"))
        .expect("expected added line");

    git_repo
        .stage_single_line(hunk, line_index, Path::new("example.txt"))
        .expect("failed to stage single line");
    let staged = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("example.txt"));

    // Refresh snapshot before unstaging to keep line coordinates in sync.
    let refreshed_snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get refreshed diff snapshot");
    let refreshed_file_change = refreshed_snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in refreshed diff");
    let refreshed_hunk = refreshed_file_change
        .hunks
        .first()
        .expect("expected refreshed hunk");
    let refreshed_line_index = refreshed_hunk
        .lines
        .iter()
        .position(|line| line.starts_with('+') && !line.starts_with("+++"))
        .expect("expected added line in refreshed hunk");

    git_repo
        .unstage_single_line(
            refreshed_hunk,
            refreshed_line_index,
            Path::new("example.txt"),
        )
        .expect("failed to unstage single line");
    let staged_after = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged_after.trim().is_empty());
}

#[test]
fn detect_staged_lines_uses_line_coordinates_not_just_content() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "one\ntwo\nthree\nfour\n");
    repo.commit_all("initial");

    // First edit: stage an added line near the top
    repo.write_file("example.txt", "one\ndup\ntwo\nthree\nfour\n");
    run_git(&repo.path, &["add", "example.txt"]);

    // Second edit: move the same content to the bottom (unstaged)
    repo.write_file("example.txt", "one\ntwo\nthree\nfour\ndup\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    // In HEAD->worktree this is an addition at the bottom. It should NOT be considered
    // staged just because the same text is staged elsewhere in the file.
    let staged_lines = git_repo
        .detect_staged_lines(hunk, Path::new("example.txt"))
        .expect("failed to detect staged lines");

    assert!(staged_lines.is_empty());
}

#[test]
fn detect_staged_lines_handles_staged_plus_with_unstaged_plus_offset() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "a\nb\nc\nd\n");
    repo.commit_all("initial");

    // Stage a change first (this becomes index state)
    repo.write_file("example.txt", "a\nb\nSTAGED\nd\n");
    run_git(&repo.path, &["add", "example.txt"]);

    // Add an unstaged line earlier in the file (worktree only)
    repo.write_file("example.txt", "a\nUNSTAGED\nb\nSTAGED\nd\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");

    // Find the hunk containing the staged +STAGED line and ensure it is detected staged.
    let mut found = false;
    for hunk in &file_change.hunks {
        if let Some(idx) = hunk
            .lines
            .iter()
            .position(|line| line.trim_end() == "+STAGED")
        {
            let staged = git_repo
                .detect_staged_lines(hunk, Path::new("example.txt"))
                .expect("failed to detect staged lines");
            assert!(
                staged.contains(&idx),
                "expected +STAGED to be detected as staged; got {:?}",
                staged
            );
            found = true;
            break;
        }
    }

    assert!(found, "expected to find hunk containing +STAGED line");
}

#[test]
fn stage_single_line_handles_existing_staged_changes_in_same_file() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "one\ntwo\nthree\nfour\n");
    repo.commit_all("initial");

    // Stage one change first
    repo.write_file("example.txt", "one\ntwo-staged\nthree\nfour\n");
    run_git(&repo.path, &["add", "example.txt"]);

    // Leave another change unstaged in the same file
    repo.write_file("example.txt", "one\ntwo-staged\nthree\nfour-unstaged\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    let add_line_index = hunk
        .lines
        .iter()
        .position(|line| line.trim_end() == "+four-unstaged")
        .expect("expected unstaged added line");

    git_repo
        .stage_single_line(hunk, add_line_index, Path::new("example.txt"))
        .expect("failed to stage single line in mixed staged/unstaged file");

    // Refresh snapshot after first line staging so line coordinates match updated index state
    let refreshed_snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get refreshed diff snapshot");
    let refreshed_file_change = refreshed_snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in refreshed diff");
    let refreshed_hunk = refreshed_file_change
        .hunks
        .first()
        .expect("expected refreshed hunk");

    let remove_line_index = refreshed_hunk
        .lines
        .iter()
        .position(|line| line.trim_end() == "-four")
        .expect("expected unstaged removed line");

    git_repo
        .stage_single_line(refreshed_hunk, remove_line_index, Path::new("example.txt"))
        .expect("failed to stage paired removal line in mixed staged/unstaged file");

    let unstaged_after = run_git(&repo.path, &["diff", "--", "example.txt"]);
    assert!(
        unstaged_after.trim().is_empty(),
        "expected no unstaged diff after staging line, got:\n{}",
        unstaged_after
    );
}

#[test]
fn stage_single_line_targets_selected_duplicate_addition() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "a\nb\nc\nd\ne\n");
    repo.commit_all("initial");

    // Add identical line content in two different places.
    repo.write_file("example.txt", "a\ndup\nb\nc\ndup\nd\ne\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    let dup_indices: Vec<usize> = hunk
        .lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| (line.trim_end() == "+dup").then_some(idx))
        .collect();
    assert!(
        dup_indices.len() >= 2,
        "expected at least two duplicate +dup lines"
    );

    // Stage the second duplicate only.
    git_repo
        .stage_single_line(hunk, dup_indices[1], Path::new("example.txt"))
        .expect("failed to stage selected duplicate line");

    let staged_diff = run_git(&repo.path, &["diff", "--cached", "--", "example.txt"]);
    let dup_count = staged_diff.matches("\n+dup\n").count();
    assert_eq!(
        dup_count, 1,
        "expected exactly one staged duplicate line, got:\n{}",
        staged_diff
    );
}

#[test]
fn toggle_hunk_stages_remaining_when_partially_staged() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "one\ntwo\nthree\nfour\n");
    repo.commit_all("initial");

    // One hunk with two modified lines.
    repo.write_file("example.txt", "one\ntwo-A\nthree\nfour-B\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    // Stage just one line first.
    let first_add = hunk
        .lines
        .iter()
        .position(|line| line.trim_end() == "+two-A")
        .expect("expected +two-A line");
    git_repo
        .stage_single_line(hunk, first_add, Path::new("example.txt"))
        .expect("failed to stage first line");

    // Refresh hunk, then toggle in hunk mode. This should stage remaining lines.
    let refreshed_snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get refreshed snapshot");
    let refreshed_file_change = refreshed_snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in refreshed diff");
    let refreshed_hunk = refreshed_file_change
        .hunks
        .first()
        .expect("expected refreshed hunk");

    let staged_now = git_repo
        .toggle_hunk_staging(refreshed_hunk, Path::new("example.txt"))
        .expect("failed to toggle partially staged hunk");
    assert!(staged_now, "expected hunk to become fully staged");

    let unstaged_after = run_git(&repo.path, &["diff", "--", "example.txt"]);
    assert!(
        unstaged_after.trim().is_empty(),
        "expected no unstaged diff after staging remaining lines, got:\n{}",
        unstaged_after
    );
}

#[test]
fn unstage_single_line_targets_selected_duplicate_addition() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "a\nb\nc\nd\ne\n");
    repo.commit_all("initial");

    // Add identical line content in two different places and stage both.
    repo.write_file("example.txt", "a\ndup\nb\nc\ndup\nd\ne\n");
    run_git(&repo.path, &["add", "example.txt"]);

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    let dup_indices: Vec<usize> = hunk
        .lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| (line.trim_end() == "+dup").then_some(idx))
        .collect();
    assert!(
        dup_indices.len() >= 2,
        "expected at least two duplicate +dup lines"
    );

    // Unstage the second duplicate only.
    git_repo
        .unstage_single_line(hunk, dup_indices[1], Path::new("example.txt"))
        .expect("failed to unstage selected duplicate line");

    let staged_diff = run_git(&repo.path, &["diff", "--cached", "--", "example.txt"]);
    let dup_count = staged_diff.matches("\n+dup\n").count();
    assert_eq!(
        dup_count, 1,
        "expected exactly one staged duplicate line after unstaging one, got:\n{}",
        staged_diff
    );
}

#[test]
fn diff_snapshot_reports_file_status() {
    let repo = TestRepo::new();
    repo.write_file("status.txt", "hello\n");
    repo.commit_all("initial");
    repo.write_file("status.txt", "hello world\n");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("status.txt"))
        .expect("expected changed file");
    assert_eq!(file.status, "Modified");
    assert!(!file.hunks.is_empty());
}

#[test]
#[ignore = "TDD regression: expected to fail until flake.lock stage_hunk behavior is fixed"]
fn regression_flake_lock_stage_hunk_from_partial_index_state() {
    let repo = TestRepo::new();

    // Base content committed in HEAD.
    let base = r#"{
"nodes": {
    "flake-utils": {
        "locked": {
            "lastModified": 1,
            "narHash": "sha256-flake-utils",
            "owner": "numtide",
            "repo": "flake-utils",
            "rev": "aaaaaaaa",
            "type": "github"
        }
    },
    "nixpkgs": {
        "locked": {
            "lastModified": 1763421233,
            "narHash": "sha256-Stk9ZYRkGrnnpyJ4eqt9eQtdFWRRIvMxpNRf4sIegnw=",
            "owner": "NixOS",
            "repo": "nixpkgs",
            "rev": "89c2b2330e733d6cdb5eae7b899326930c2c0648",
            "type": "github"
        },
        "original": {
            "owner": "NixOS",
            "ref": "nixos-unstable",
            "repo": "nixpkgs",
            "type": "github"
        }
    },
    "nixpkgs_2": {
        "locked": {
            "lastModified": 1744536153,
            "narHash": "sha256-awS2zRgF4uTwrOKwwiJcByDzDOdo3Q1rPZbiHQg/N38=",
            "owner": "NixOS",
            "repo": "nixpkgs",
            "rev": "18dd725c29603f582cf1900e0d25f9f1063dbf11",
            "type": "github"
        },
        "original": {
            "owner": "NixOS",
            "ref": "nixpkgs-unstable",
            "repo": "nixpkgs",
            "type": "github"
        }
    },
    "root": {
        "inputs": {
            "flake-utils": "flake-utils",
            "nixpkgs": "nixpkgs",
            "rust-overlay": "rust-overlay"
        }
    },
    "rust-overlay": {
        "inputs": {
            "nixpkgs": "nixpkgs_2"
        },
        "locked": {
            "lastModified": 1763519912,
            "narHash": "sha256-N2YN0ZNBoz2zRRjmATePp9GbmGSGpVh3+piXn6mtgKc=",
            "owner": "oxalica",
            "repo": "rust-overlay",
            "rev": "a9c35d6e7cb70c5719170b6c2d3bb589c5e048af",
            "type": "github"
        },
        "original": {
            "owner": "oxalica",
            "repo": "rust-overlay",
            "type": "github"
        }
    }
},
"root": "root",
"version": 7
}
"#;

    // Staged/index content from problematic session (contains duplicated keys).
    let indexed = r#"{
"nodes": {
    "flake-utils": {
        "locked": {
            "lastModified": 1,
            "narHash": "sha256-flake-utils",
            "owner": "numtide",
            "repo": "flake-utils",
            "rev": "aaaaaaaa",
            "type": "github"
        }
    },
    "nixpkgs": {
        "locked": {
            "lastModified": 1763421233,
            "lastModified": 1763421233,
            "narHash": "sha256-Stk9ZYRkGrnnpyJ4eqt9eQtdFWRRIvMxpNRf4sIegnw=",
            "lastModified": 1763421233,
            "narHash": "sha256-Stk9ZYRkGrnnpyJ4eqt9eQtdFWRRIvMxpNRf4sIegnw=",
            "narHash": "sha256-Stk9ZYRkGrnnpyJ4eqt9eQtdFWRRIvMxpNRf4sIegnw=",
            "rev": "89c2b2330e733d6cdb5eae7b899326930c2c0648",
            "lastModified": 1763421233,
            "lastModified": 1763421233,
            "rev": "89c2b2330e733d6cdb5eae7b899326930c2c0648",
            "rev": "89c2b2330e733d6cdb5eae7b899326930c2c0648",
            "lastModified": 1763421233,
            "lastModified": 1763421233,
            "narHash": "sha256-Stk9ZYRkGrnnpyJ4eqt9eQtdFWRRIvMxpNRf4sIegnw=",
            "owner": "NixOS",
            "repo": "nixpkgs",
            "rev": "89c2b2330e733d6cdb5eae7b899326930c2c0648",
            "type": "github"
        },
        "original": {
            "owner": "NixOS",
            "ref": "nixos-unstable",
            "repo": "nixpkgs",
            "type": "github"
        }
    },
    "nixpkgs_2": {
        "locked": {
            "lastModified": 1744536153,
            "narHash": "sha256-awS2zRgF4uTwrOKwwiJcByDzDOdo3Q1rPZbiHQg/N38=",
            "owner": "NixOS",
            "repo": "nixpkgs",
            "rev": "18dd725c29603f582cf1900e0d25f9f1063dbf11",
            "type": "github"
        },
        "original": {
            "owner": "NixOS",
            "ref": "nixpkgs-unstable",
            "repo": "nixpkgs",
            "type": "github"
        }
    },
    "root": {
        "inputs": {
            "flake-utils": "flake-utils",
            "nixpkgs": "nixpkgs",
            "rust-overlay": "rust-overlay"
        }
    },
    "rust-overlay": {
        "inputs": {
            "nixpkgs": "nixpkgs_2"
        },
        "locked": {
            "lastModified": 1763519912,
            "narHash": "sha256-N2YN0ZNBoz2zRRjmATePp9GbmGSGpVh3+piXn6mtgKc=",
            "owner": "oxalica",
            "repo": "rust-overlay",
            "rev": "a9c35d6e7cb70c5719170b6c2d3bb589c5e048af",
            "type": "github"
        },
        "original": {
            "owner": "oxalica",
            "repo": "rust-overlay",
            "type": "github"
        }
    }
},
"root": "root",
"version": 7
}
"#;

    // Worktree content (unstaged) from problematic session.
    let worktree = r#"{
"nodes": {
    "flake-utils": {
        "locked": {
            "lastModified": 1,
            "narHash": "sha256-flake-utils",
            "owner": "numtide",
            "repo": "flake-utils",
            "rev": "aaaaaaaa",
            "type": "github"
        }
    },
    "nixpkgs": {
        "locked": {
            "lastModified": 1771369470,
            "narHash": "sha256-0NBlEBKkN3lufyvFegY4TYv5mCNHbi5OmBDrzihbBMQ=",
            "owner": "NixOS",
            "repo": "nixpkgs",
            "rev": "0182a361324364ae3f436a63005877674cf45efb",
            "type": "github"
        },
        "original": {
            "owner": "NixOS",
            "ref": "nixos-unstable",
            "repo": "nixpkgs",
            "type": "github"
        }
    },
    "root": {
        "inputs": {
            "flake-utils": "flake-utils",
            "nixpkgs": "nixpkgs",
            "rust-overlay": "rust-overlay"
        }
    },
    "rust-overlay": {
        "inputs": {
            "nixpkgs": [
                "nixpkgs"
            ]
        },
        "locked": {
            "lastModified": 1771556776,
            "narHash": "sha256-zKprqMQDl3xVfhSSYvgru1IGXjFdxryWk+KqK0I20Xk=",
            "owner": "oxalica",
            "repo": "rust-overlay",
            "rev": "8b3f46b8a6d17ab46e533a5e3d5b1cc2ff228860",
            "type": "github"
        },
        "original": {
            "owner": "oxalica",
            "repo": "rust-overlay",
            "type": "github"
        }
    }
},
"root": "root",
"version": 7
}
"#;

    repo.write_file("flake.lock", base);
    repo.commit_all("initial");

    // Set index to problematic staged state.
    repo.write_file("flake.lock", indexed);
    run_git(&repo.path, &["add", "flake.lock"]);

    // Set worktree to problematic unstaged state.
    repo.write_file("flake.lock", worktree);

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|file| file.path == PathBuf::from("flake.lock"))
        .expect("expected flake.lock in diff");
    let hunk = file_change
        .hunks
        .first()
        .expect("expected hunk in flake.lock");

    // TDD target: this should succeed once fixed. It currently fails with
    // "patch does not apply" for flake.lock around old_start=20.
    git_repo
        .stage_hunk(hunk, Path::new("flake.lock"))
        .expect("stage_hunk should succeed for this flake.lock state");
}

#[test]
fn get_recent_commits_returns_commit_list() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit_all("first commit");
    repo.write_file("a.txt", "two\n");
    repo.commit_all("second commit");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let commits = git_repo
        .get_recent_commits(10)
        .expect("failed to get recent commits");

    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].summary, "second commit");
    assert_eq!(commits[1].summary, "first commit");
    assert_eq!(commits[0].short_sha.len(), 7);
    assert_eq!(commits[0].author, "Test User");
}

#[test]
fn get_recent_commits_respects_count_limit() {
    let repo = TestRepo::new();
    for i in 0..5 {
        repo.write_file("a.txt", &format!("content {}\n", i));
        repo.commit_all(&format!("commit {}", i));
    }

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let commits = git_repo
        .get_recent_commits(3)
        .expect("failed to get recent commits");

    assert_eq!(commits.len(), 3);
}

#[test]
fn get_commit_diff_returns_snapshot_for_commit() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline 2 updated\n");
    repo.commit_all("modify line 2");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let commits = git_repo
        .get_recent_commits(2)
        .expect("failed to get commits");
    let latest_sha = &commits[0].sha;

    let snapshot = git_repo
        .get_commit_diff(latest_sha)
        .expect("failed to get commit diff");

    assert!(!snapshot.files.is_empty());
    let file = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected example.txt in commit diff");
    assert!(!file.hunks.is_empty());

    // Check that the diff contains expected changes
    let hunk = &file.hunks[0];
    let has_removal = hunk
        .lines
        .iter()
        .any(|l| l.starts_with('-') && l.contains("line 2"));
    let has_addition = hunk
        .lines
        .iter()
        .any(|l| l.starts_with('+') && l.contains("line 2 updated"));
    assert!(has_removal, "expected removal of 'line 2'");
    assert!(has_addition, "expected addition of 'line 2 updated'");
}

#[test]
fn get_commit_diff_handles_initial_commit() {
    let repo = TestRepo::new();
    repo.write_file("new.txt", "brand new\n");
    repo.commit_all("initial commit");

    let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
    let commits = git_repo
        .get_recent_commits(1)
        .expect("failed to get commits");

    let snapshot = git_repo
        .get_commit_diff(&commits[0].sha)
        .expect("failed to get initial commit diff");

    assert!(!snapshot.files.is_empty());
}
