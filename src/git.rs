use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository};
use std::path::{Path, PathBuf};

use crate::diff::{DiffSnapshot, FileChange, Hunk};

#[derive(Clone)]
pub struct GitRepo {
    repo_path: PathBuf,
}

impl GitRepo {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo_path = Repository::discover(path.as_ref())
            .context("Failed to find git repository")?
            .workdir()
            .context("Repository has no working directory")?
            .to_path_buf();
        
        Ok(Self { repo_path })
    }
    
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
    
    pub fn get_diff_snapshot(&self) -> Result<DiffSnapshot> {
        let repo = Repository::open(&self.repo_path)?;
        
        // Get the diff between HEAD and working directory (includes both staged and unstaged)
        let mut diff_opts = DiffOptions::new();
        diff_opts.include_untracked(true);
        diff_opts.recurse_untracked_dirs(true);
        
        // Get HEAD tree (handle empty repo case)
        let head_tree = match repo.head() {
            Ok(head) => head.peel_to_tree().ok(),
            Err(_) => None,
        };
        
        // This shows all changes from HEAD to workdir (both staged and unstaged)
        let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;
        
        let mut files = Vec::new();
        
        diff.foreach(
            &mut |delta, _progress| {
                let file_path = match delta.status() {
                    Delta::Added | Delta::Modified | Delta::Deleted => {
                        delta.new_file().path()
                            .or_else(|| delta.old_file().path())
                    }
                    _ => None,
                };
                
                if let Some(path) = file_path {
                    files.push(FileChange {
                        path: path.to_path_buf(),
                        status: format!("{:?}", delta.status()),
                        hunks: Vec::new(),
                    });
                }
                true
            },
            None,
            None,
            None,
        )?;
        
        // Now get the actual diff content for each file
        for file in &mut files {
            if let Ok(hunks) = self.get_file_hunks(&repo, &file.path) {
                file.hunks = hunks;
            }
        }
        
        Ok(DiffSnapshot {
            timestamp: std::time::SystemTime::now(),
            files,
        })
    }
    
    fn get_file_hunks(&self, repo: &Repository, path: &Path) -> Result<Vec<Hunk>> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(path);
        diff_opts.context_lines(3);
        
        // Get HEAD tree (handle empty repo case)
        let head_tree = match repo.head() {
            Ok(head) => head.peel_to_tree().ok(),
            Err(_) => None,
        };
        
        // Get diff from HEAD to workdir (includes both staged and unstaged)
        let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;
        
        let path_buf = path.to_path_buf();
        
        use std::cell::RefCell;
        use std::rc::Rc;
        
        let hunks = Rc::new(RefCell::new(Vec::new()));
        let current_hunk_lines = Rc::new(RefCell::new(Vec::new()));
        let current_old_start = Rc::new(RefCell::new(0usize));
        let current_new_start = Rc::new(RefCell::new(0usize));
        let in_hunk = Rc::new(RefCell::new(false));
        
        let hunks_clone = hunks.clone();
        let lines_clone = current_hunk_lines.clone();
        let old_clone = current_old_start.clone();
        let new_clone = current_new_start.clone();
        let in_hunk_clone = in_hunk.clone();
        let path_clone = path_buf.clone();
        
        let lines_clone2 = current_hunk_lines.clone();
        let in_hunk_clone2 = in_hunk.clone();
        
        diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut move |_, hunk| {
                // Save previous hunk if exists
                if *in_hunk_clone.borrow() && !lines_clone.borrow().is_empty() {
                    hunks_clone.borrow_mut().push(Hunk::new(
                        *old_clone.borrow(),
                        *new_clone.borrow(),
                        lines_clone.borrow().clone(),
                        &path_clone
                    ));
                    lines_clone.borrow_mut().clear();
                }
                
                // Start new hunk
                *old_clone.borrow_mut() = hunk.old_start() as usize;
                *new_clone.borrow_mut() = hunk.new_start() as usize;
                *in_hunk_clone.borrow_mut() = true;
                true
            }),
            Some(&mut move |_, _, line| {
                // Add line to current hunk
                if *in_hunk_clone2.borrow() {
                    let content = String::from_utf8_lossy(line.content()).to_string();
                    lines_clone2.borrow_mut().push(format!("{}{}", line.origin(), content));
                }
                true
            }),
        )?;
        
        // Don't forget the last hunk
        if *in_hunk.borrow() && !current_hunk_lines.borrow().is_empty() {
            hunks.borrow_mut().push(Hunk::new(
                *current_old_start.borrow(),
                *current_new_start.borrow(),
                current_hunk_lines.borrow().clone(),
                &path_buf
            ));
        }
        
        // Extract the hunks - clone to avoid lifetime issues
        let result = hunks.borrow().clone();
        Ok(result)
    }
    
    /// Stage an entire file
    pub fn stage_file(&self, file_path: &Path) -> Result<()> {
        let repo = Repository::open(&self.repo_path)?;
        let mut index = repo.index()?;
        index.add_path(file_path)?;
        index.write()?;
        Ok(())
    }
    
    /// Stage a specific hunk by applying it as a patch
    pub fn stage_hunk(&self, hunk: &Hunk, file_path: &Path) -> Result<()> {
        use std::process::Command;
        use std::io::Write;
        
        // Create a proper unified diff patch
        let mut patch = String::new();
        
        // Diff header
        patch.push_str(&format!("diff --git a/{} b/{}\n", file_path.display(), file_path.display()));
        patch.push_str(&format!("--- a/{}\n", file_path.display()));
        patch.push_str(&format!("+++ b/{}\n", file_path.display()));
        
        // Count actual add/remove lines for the hunk header
        let mut old_lines = 0;
        let mut new_lines = 0;
        for line in &hunk.lines {
            if line.starts_with('-') && !line.starts_with("---") {
                old_lines += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                new_lines += 1;
            } else if line.starts_with(' ') {
                old_lines += 1;
                new_lines += 1;
            }
        }
        
        // Hunk header
        patch.push_str(&format!("@@ -{},{} +{},{} @@\n", 
            hunk.old_start, 
            old_lines, 
            hunk.new_start, 
            new_lines
        ));
        
        // Hunk content
        for line in &hunk.lines {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }
        
        // Use git apply to stage the hunk
        let mut child = Command::new("git")
            .arg("apply")
            .arg("--cached")
            .arg("--unidiff-zero")
            .arg("-")
            .current_dir(&self.repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(patch.as_bytes())?;
        }
        
        let output = child.wait_with_output()?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to stage hunk: {}", error_msg));
        }
        
        Ok(())
    }
    
    /// Detect which lines in a hunk are currently staged in the index
    /// Returns a HashSet of line indices that are staged
    pub fn detect_staged_lines(&self, hunk: &Hunk, file_path: &Path) -> Result<std::collections::HashSet<usize>> {
        use std::collections::HashSet;
        
        let repo = Repository::open(&self.repo_path)?;
        
        // Get diff from HEAD to index (only staged changes)
        let head_tree = match repo.head() {
            Ok(head) => head.peel_to_tree().ok(),
            Err(_) => None,
        };
        
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(file_path);
        
        let diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut diff_opts))?;
        
        let mut staged_lines = HashSet::new();
        
        // Collect all hunks from the staged diff with their line ranges
        use std::cell::RefCell;
        let staged_hunks = RefCell::new(Vec::new());
        
        diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, diff_hunk| {
                // Store the hunk's old_start to identify it
                staged_hunks.borrow_mut().push((diff_hunk.old_start() as usize, Vec::new()));
                true
            }),
            Some(&mut |_, _, line| {
                let content = String::from_utf8_lossy(line.content()).to_string();
                let line_str = format!("{}{}", line.origin(), content);
                
                // Add to the most recent hunk
                if let Some(last_hunk) = staged_hunks.borrow_mut().last_mut() {
                    last_hunk.1.push(line_str);
                }
                true
            }),
        )?;
        
        let staged_hunks = staged_hunks.into_inner();
        
        // Find the matching staged hunk by old_start position
        let matching_staged_hunk = staged_hunks.iter()
            .find(|(old_start, _)| *old_start == hunk.old_start);
        
        if let Some((_, staged_hunk_lines)) = matching_staged_hunk {
            // Match lines from our hunk with the staged hunk's lines
            for (hunk_idx, hunk_line) in hunk.lines.iter().enumerate() {
                // Only check change lines (+ or -)
                if (hunk_line.starts_with('+') && !hunk_line.starts_with("+++")) ||
                   (hunk_line.starts_with('-') && !hunk_line.starts_with("---")) {
                    // Check if this exact line exists in the matching staged hunk
                    if staged_hunk_lines.iter().any(|staged_line| {
                        staged_line.trim_end() == hunk_line.trim_end()
                    }) {
                        staged_lines.insert(hunk_idx);
                    }
                }
            }
        }
        
        Ok(staged_lines)
    }
    
    /// Stage a single line from a hunk
    pub fn stage_single_line(&self, hunk: &Hunk, line_index: usize, file_path: &Path) -> Result<()> {
        use std::process::Command;
        use std::io::Write;
        
        // Verify the line exists
        if line_index >= hunk.lines.len() {
            return Err(anyhow::anyhow!("Line index out of bounds"));
        }
        
        let selected_line = &hunk.lines[line_index];
        
        // Only allow staging change lines
        if !((selected_line.starts_with('+') && !selected_line.starts_with("+++")) ||
             (selected_line.starts_with('-') && !selected_line.starts_with("---"))) {
            return Err(anyhow::anyhow!("Can only stage + or - lines"));
        }
        
        // For now, let's use a simpler approach: stage the whole hunk
        // In a production implementation, you'd want to use git add --interactive style patching
        // or use libgit2's apply functionality with more precise patches
        
        // Create a patch with just this single line change
        let mut patch = String::new();
        
        // Diff header
        patch.push_str(&format!("diff --git a/{} b/{}\n", file_path.display(), file_path.display()));
        patch.push_str(&format!("--- a/{}\n", file_path.display()));
        patch.push_str(&format!("+++ b/{}\n", file_path.display()));
        
        // For single-line staging, we need proper context from the hunk
        // Find all context lines around our target line
        let mut context_before = Vec::new();
        let mut context_after = Vec::new();
        
        // Collect context before the selected line
        let mut i = line_index;
        while i > 0 && context_before.len() < 3 {
            i -= 1;
            let line = &hunk.lines[i];
            if line.starts_with(' ') {
                context_before.insert(0, line.clone());
            } else {
                // Hit another change line, stop
                break;
            }
        }
        
        // Collect context after the selected line
        let mut i = line_index + 1;
        while i < hunk.lines.len() && context_after.len() < 3 {
            let line = &hunk.lines[i];
            if line.starts_with(' ') {
                context_after.push(line.clone());
                i += 1;
            } else {
                // Hit another change line, stop
                break;
            }
        }
        
        // Calculate line numbers for the hunk header
        // This is approximate - we're counting context lines to estimate position
        let is_addition = selected_line.starts_with('+');
        let context_before_count = context_before.len();
        
        let old_line_count = context_before_count + if is_addition { 0 } else { 1 } + context_after.len();
        let new_line_count = context_before_count + if is_addition { 1 } else { 0 } + context_after.len();
        
        // Estimate old_start and new_start (this is approximate)
        let estimated_old_start = hunk.old_start + line_index - context_before_count;
        let estimated_new_start = hunk.new_start + line_index - context_before_count;
        
        // Write hunk header
        patch.push_str(&format!("@@ -{},{} +{},{} @@\n",
            estimated_old_start,
            old_line_count,
            estimated_new_start,
            new_line_count
        ));
        
        // Write context before
        for line in &context_before {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }
        
        // Write the selected line
        patch.push_str(selected_line);
        if !selected_line.ends_with('\n') {
            patch.push('\n');
        }
        
        // Write context after
        for line in &context_after {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }
        
        // Try to apply the patch
        let mut child = Command::new("git")
            .arg("apply")
            .arg("--cached")
            .arg("--unidiff-zero")
            .arg("--allow-overlap")
            .arg("-")
            .current_dir(&self.repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(patch.as_bytes())?;
        }
        
        let output = child.wait_with_output()?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            let patch_preview = if patch.len() > 500 {
                format!("{}... (truncated)", &patch[..500])
            } else {
                patch.clone()
            };
            return Err(anyhow::anyhow!("Failed to stage line: {}\nPatch was:\n{}", error_msg, patch_preview));
        }
        
        Ok(())
    }
    
    /// Unstage a single line from a hunk
    pub fn unstage_single_line(&self, hunk: &Hunk, line_index: usize, file_path: &Path) -> Result<()> {
        use std::process::Command;
        use std::io::Write;
        
        // Verify the line exists
        if line_index >= hunk.lines.len() {
            return Err(anyhow::anyhow!("Line index out of bounds"));
        }
        
        let selected_line = &hunk.lines[line_index];
        
        // Only allow unstaging change lines
        if !((selected_line.starts_with('+') && !selected_line.starts_with("+++")) ||
             (selected_line.starts_with('-') && !selected_line.starts_with("---"))) {
            return Err(anyhow::anyhow!("Can only unstage + or - lines"));
        }
        
        // Create a reverse patch to unstage the line
        // For unstaging, we need to reverse the operation:
        // - If the line is "+something", we remove it from the index (reverse: "-something")
        // - If the line is "-something", we add it back to the index (reverse: "+something")
        
        let mut patch = String::new();
        
        // Diff header
        patch.push_str(&format!("diff --git a/{} b/{}\n", file_path.display(), file_path.display()));
        patch.push_str(&format!("--- a/{}\n", file_path.display()));
        patch.push_str(&format!("+++ b/{}\n", file_path.display()));
        
        // Find context lines around the target line
        let mut context_before = Vec::new();
        let mut context_after = Vec::new();
        
        // Collect context before the selected line
        let mut i = line_index;
        while i > 0 && context_before.len() < 3 {
            i -= 1;
            let line = &hunk.lines[i];
            if line.starts_with(' ') {
                context_before.insert(0, line.clone());
            } else {
                break;
            }
        }
        
        // Collect context after the selected line
        let mut i = line_index + 1;
        while i < hunk.lines.len() && context_after.len() < 3 {
            let line = &hunk.lines[i];
            if line.starts_with(' ') {
                context_after.push(line.clone());
                i += 1;
            } else {
                break;
            }
        }
        
        // For unstaging, we apply the SAME patch as staging but with --reverse flag
        // Don't manually reverse the line - git apply --reverse will do that
        
        // Calculate line numbers for the hunk header
        let is_addition = selected_line.starts_with('+');
        let context_before_count = context_before.len();
        
        let old_line_count = context_before_count + if is_addition { 0 } else { 1 } + context_after.len();
        let new_line_count = context_before_count + if is_addition { 1 } else { 0 } + context_after.len();
        
        let estimated_old_start = hunk.old_start + line_index - context_before_count;
        let estimated_new_start = hunk.new_start + line_index - context_before_count;
        
        // Write hunk header
        patch.push_str(&format!("@@ -{},{} +{},{} @@\n",
            estimated_old_start,
            old_line_count,
            estimated_new_start,
            new_line_count
        ));
        
        // Write context before
        for line in &context_before {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }
        
        // Write the selected line (not reversed - git apply --reverse will handle that)
        patch.push_str(selected_line);
        if !selected_line.ends_with('\n') {
            patch.push('\n');
        }
        
        // Write context after
        for line in &context_after {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }
        
        // Apply the reverse patch to the index using --cached and --reverse
        let mut child = Command::new("git")
            .arg("apply")
            .arg("--cached")
            .arg("--reverse")
            .arg("--unidiff-zero")
            .arg("--allow-overlap")
            .arg("-")
            .current_dir(&self.repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(patch.as_bytes())?;
        }
        
        let output = child.wait_with_output()?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            let patch_preview = if patch.len() > 500 {
                format!("{}... (truncated)", &patch[..500])
            } else {
                patch.clone()
            };
            return Err(anyhow::anyhow!("Failed to unstage line: {}\nPatch was:\n{}", error_msg, patch_preview));
        }
        
        Ok(())
    }
    
    /// Unstage an entire file
    pub fn unstage_file(&self, file_path: &Path) -> Result<()> {
        use std::process::Command;
        
        let output = Command::new("git")
            .arg("reset")
            .arg("HEAD")
            .arg("--")
            .arg(file_path)
            .current_dir(&self.repo_path)
            .output()?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to unstage file: {}", error_msg));
        }
        
        Ok(())
    }
    
    /// Unstage a specific hunk by applying the reverse patch
    pub fn unstage_hunk(&self, hunk: &Hunk, file_path: &Path) -> Result<()> {
        use std::process::Command;
        use std::io::Write;
        
        // Create a proper unified diff patch
        let mut patch = String::new();
        
        // Diff header
        patch.push_str(&format!("diff --git a/{} b/{}\n", file_path.display(), file_path.display()));
        patch.push_str(&format!("--- a/{}\n", file_path.display()));
        patch.push_str(&format!("+++ b/{}\n", file_path.display()));
        
        // Count actual add/remove lines for the hunk header
        let mut old_lines = 0;
        let mut new_lines = 0;
        for line in &hunk.lines {
            if line.starts_with('-') && !line.starts_with("---") {
                old_lines += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                new_lines += 1;
            } else if line.starts_with(' ') {
                old_lines += 1;
                new_lines += 1;
            }
        }
        
        // Hunk header
        patch.push_str(&format!("@@ -{},{} +{},{} @@\n", 
            hunk.old_start, 
            old_lines, 
            hunk.new_start, 
            new_lines
        ));
        
        // Hunk content
        for line in &hunk.lines {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }
        
        // Use git apply --reverse to unstage the hunk
        let mut child = Command::new("git")
            .arg("apply")
            .arg("--cached")
            .arg("--reverse")
            .arg("--unidiff-zero")
            .arg("-")
            .current_dir(&self.repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(patch.as_bytes())?;
        }
        
        let output = child.wait_with_output()?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to unstage hunk: {}", error_msg));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
                .expect("system time before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("hunky-git-tests-{}-{}", std::process::id(), unique));

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
    fn stage_and_unstage_file_updates_index() {
        let repo = TestRepo::new();
        repo.write_file("example.txt", "line 1\nline 2\n");
        repo.commit_all("initial");
        repo.write_file("example.txt", "line 1\nline 2\nline 3\n");

        let git_repo = GitRepo::new(&repo.path).expect("failed to open test repo");
        let file_path = Path::new("example.txt");

        git_repo.stage_file(file_path).expect("failed to stage file");
        let staged = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
        assert!(staged.contains("example.txt"));

        git_repo
            .unstage_file(file_path)
            .expect("failed to unstage file");
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
}
