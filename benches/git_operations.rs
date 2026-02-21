use criterion::{criterion_group, criterion_main, Criterion};
use hunky::git::GitRepo;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct BenchRepo {
    path: PathBuf,
}

impl BenchRepo {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("failed to get system time")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("hunky-bench-{}-{}", std::process::id(), unique));

        fs::create_dir_all(&path).expect("failed to create temp directory");

        run_git(&path, &["init"]);
        run_git(&path, &["config", "user.name", "Bench User"]);
        run_git(&path, &["config", "user.email", "bench@example.com"]);

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

impl Drop for BenchRepo {
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

    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Create a repo with a committed file and an unstaged modification.
fn setup_modified_repo() -> BenchRepo {
    let repo = BenchRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\nline 3\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline 2 modified\nline 3\n");
    repo
}

/// Create a repo with a committed file and a larger unstaged modification.
fn setup_large_modified_repo() -> BenchRepo {
    let repo = BenchRepo::new();
    let mut base = String::new();
    for i in 0..100 {
        base.push_str(&format!("line {}\n", i));
    }
    repo.write_file("large.txt", &base);
    repo.commit_all("initial");

    let mut modified = String::new();
    for i in 0..100 {
        if i % 10 == 0 {
            modified.push_str(&format!("modified line {}\n", i));
        } else {
            modified.push_str(&format!("line {}\n", i));
        }
    }
    repo.write_file("large.txt", &modified);
    repo
}

fn bench_get_diff_snapshot(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");

    c.bench_function("get_diff_snapshot", |b| {
        b.iter(|| {
            git_repo
                .get_diff_snapshot()
                .expect("failed to get diff snapshot");
        });
    });
}

fn bench_get_diff_snapshot_large(c: &mut Criterion) {
    let repo = setup_large_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");

    c.bench_function("get_diff_snapshot_large", |b| {
        b.iter(|| {
            git_repo
                .get_diff_snapshot()
                .expect("failed to get diff snapshot");
        });
    });
}

fn bench_stage_file(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");
    let file_path = Path::new("example.txt");

    c.bench_function("stage_file", |b| {
        b.iter(|| {
            git_repo
                .stage_file(file_path)
                .expect("failed to stage file");
            // Reset for next iteration
            run_git(&repo.path, &["reset", "HEAD", "example.txt"]);
        });
    });
}

fn bench_unstage_file(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");
    let file_path = Path::new("example.txt");

    c.bench_function("unstage_file", |b| {
        b.iter(|| {
            // Stage first, then measure unstage
            run_git(&repo.path, &["add", "example.txt"]);
            git_repo
                .unstage_file(file_path)
                .expect("failed to unstage file");
        });
    });
}

fn bench_stage_hunk(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");
    let file_path = Path::new("example.txt");

    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    c.bench_function("stage_hunk", |b| {
        b.iter(|| {
            git_repo
                .stage_hunk(hunk, file_path)
                .expect("failed to stage hunk");
            run_git(&repo.path, &["reset", "HEAD", "example.txt"]);
        });
    });
}

fn bench_unstage_hunk(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");
    let file_path = Path::new("example.txt");

    // Pre-compute the hunk from the staged state
    run_git(&repo.path, &["add", "example.txt"]);
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk").clone();

    c.bench_function("unstage_hunk", |b| {
        b.iter(|| {
            run_git(&repo.path, &["add", "example.txt"]);
            git_repo
                .unstage_hunk(&hunk, file_path)
                .expect("failed to unstage hunk");
        });
    });
}

fn bench_stage_single_line(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");

    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");
    let line_index = hunk
        .lines
        .iter()
        .position(|line| line.starts_with('+') && !line.starts_with("+++"))
        .expect("expected added line");

    c.bench_function("stage_single_line", |b| {
        b.iter(|| {
            git_repo
                .stage_single_line(hunk, line_index, Path::new("example.txt"))
                .expect("failed to stage single line");
            run_git(&repo.path, &["reset", "HEAD", "example.txt"]);
        });
    });
}

fn bench_unstage_single_line(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");

    // Pre-compute the hunk from the staged state
    run_git(&repo.path, &["add", "example.txt"]);
    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk").clone();
    let line_index = hunk
        .lines
        .iter()
        .position(|line| line.starts_with('+') && !line.starts_with("+++"))
        .expect("expected added line");

    c.bench_function("unstage_single_line", |b| {
        b.iter(|| {
            run_git(&repo.path, &["add", "example.txt"]);
            git_repo
                .unstage_single_line(&hunk, line_index, Path::new("example.txt"))
                .expect("failed to unstage single line");
        });
    });
}

fn bench_detect_staged_lines(c: &mut Criterion) {
    let repo = setup_modified_repo();
    // Stage the file so there are staged lines to detect
    run_git(&repo.path, &["add", "example.txt"]);
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");

    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    c.bench_function("detect_staged_lines", |b| {
        b.iter(|| {
            git_repo
                .detect_staged_lines(hunk, Path::new("example.txt"))
                .expect("failed to detect staged lines");
        });
    });
}

fn bench_toggle_hunk_staging(c: &mut Criterion) {
    let repo = setup_modified_repo();
    let git_repo = GitRepo::new(&repo.path).expect("failed to open repo");

    let snapshot = git_repo
        .get_diff_snapshot()
        .expect("failed to get diff snapshot");
    let file_change = snapshot
        .files
        .iter()
        .find(|f| f.path == PathBuf::from("example.txt"))
        .expect("expected file in diff");
    let hunk = file_change.hunks.first().expect("expected hunk");

    c.bench_function("toggle_hunk_staging", |b| {
        b.iter(|| {
            git_repo
                .toggle_hunk_staging(hunk, Path::new("example.txt"))
                .expect("failed to toggle hunk staging");
            run_git(&repo.path, &["reset", "HEAD", "example.txt"]);
        });
    });
}

criterion_group!(
    benches,
    bench_get_diff_snapshot,
    bench_get_diff_snapshot_large,
    bench_stage_file,
    bench_unstage_file,
    bench_stage_hunk,
    bench_unstage_hunk,
    bench_stage_single_line,
    bench_unstage_single_line,
    bench_detect_staged_lines,
    bench_toggle_hunk_staging,
);
criterion_main!(benches);
