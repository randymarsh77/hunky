use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository};
use std::path::{Path, PathBuf};

use crate::diff::{DiffSnapshot, FileChange, Hunk};

#[derive(Clone)]
pub struct GitRepo {
    repo_path: PathBuf,
}

impl GitRepo {
    fn build_single_line_patch(&self, hunk: &Hunk, line_index: usize, file_path: &Path) -> Result<String> {
        // Verify the line exists
        if line_index >= hunk.lines.len() {
            return Err(anyhow::anyhow!("Line index out of bounds"));
        }

        let selected_line = &hunk.lines[line_index];

        // Only allow patching change lines
        if !((selected_line.starts_with('+') && !selected_line.starts_with("+++"))
            || (selected_line.starts_with('-') && !selected_line.starts_with("---")))
        {
            return Err(anyhow::anyhow!("Can only patch + or - lines"));
        }

        // Collect local context around the selected line (only unchanged context lines)
        let mut context_before = Vec::new();
        let mut context_after = Vec::new();

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

        // Compute exact old/new start positions for the first line included in this mini-hunk.
        // This avoids the previous approximation that used raw vector index offsets.
        let start_idx = line_index.saturating_sub(context_before.len());
        let mut old_start = hunk.old_start;
        let mut new_start = hunk.new_start;

        for line in hunk.lines.iter().take(start_idx) {
            if line.starts_with(' ') {
                old_start += 1;
                new_start += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                old_start += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                new_start += 1;
            }
        }

        // Build the lines included in this mini-hunk and derive exact old/new counts
        let mut mini_hunk_lines = Vec::new();
        mini_hunk_lines.extend(context_before.clone());
        mini_hunk_lines.push(selected_line.clone());
        mini_hunk_lines.extend(context_after.clone());

        let mut old_line_count = 0;
        let mut new_line_count = 0;
        for line in &mini_hunk_lines {
            if line.starts_with(' ') {
                old_line_count += 1;
                new_line_count += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                old_line_count += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                new_line_count += 1;
            }
        }

        // Create a proper unified diff patch
        let mut patch = String::new();
        patch.push_str(&format!(
            "diff --git a/{} b/{}\n",
            file_path.display(),
            file_path.display()
        ));
        patch.push_str(&format!("--- a/{}\n", file_path.display()));
        patch.push_str(&format!("+++ b/{}\n", file_path.display()));
        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start, old_line_count, new_start, new_line_count
        ));

        for line in &mini_hunk_lines {
            patch.push_str(line);
            if !line.ends_with('\n') {
                patch.push('\n');
            }
        }

        Ok(patch)
    }

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

    fn get_unstaged_file_hunks(&self, repo: &Repository, path: &Path) -> Result<Vec<Hunk>> {
        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(path);
        diff_opts.context_lines(3);

        let index = repo.index()?;
        let diff = repo.diff_index_to_workdir(Some(&index), Some(&mut diff_opts))?;

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
                if *in_hunk_clone.borrow() && !lines_clone.borrow().is_empty() {
                    hunks_clone.borrow_mut().push(Hunk::new(
                        *old_clone.borrow(),
                        *new_clone.borrow(),
                        lines_clone.borrow().clone(),
                        &path_clone,
                    ));
                    lines_clone.borrow_mut().clear();
                }

                *old_clone.borrow_mut() = hunk.old_start() as usize;
                *new_clone.borrow_mut() = hunk.new_start() as usize;
                *in_hunk_clone.borrow_mut() = true;
                true
            }),
            Some(&mut move |_, _, line| {
                if *in_hunk_clone2.borrow() {
                    let content = String::from_utf8_lossy(line.content()).to_string();
                    lines_clone2.borrow_mut().push(format!("{}{}", line.origin(), content));
                }
                true
            }),
        )?;

        if *in_hunk.borrow() && !current_hunk_lines.borrow().is_empty() {
            hunks.borrow_mut().push(Hunk::new(
                *current_old_start.borrow(),
                *current_new_start.borrow(),
                current_hunk_lines.borrow().clone(),
                &path_buf,
            ));
        }

        let result = hunks.borrow().clone();
        Ok(result)
    }

    fn resolve_line_against_unstaged_diff(
        &self,
        hunk: &Hunk,
        line_index: usize,
        file_path: &Path,
    ) -> Result<(Hunk, usize)> {
        let selected_line = hunk
            .lines
            .get(line_index)
            .ok_or_else(|| anyhow::anyhow!("Line index out of bounds"))?
            .trim_end()
            .to_string();

        let repo = Repository::open(&self.repo_path)?;
        let unstaged_hunks = self.get_unstaged_file_hunks(&repo, file_path)?;

        for unstaged_hunk in unstaged_hunks {
            for (idx, line) in unstaged_hunk.lines.iter().enumerate() {
                let is_change_line = (line.starts_with('+') && !line.starts_with("+++"))
                    || (line.starts_with('-') && !line.starts_with("---"));

                if is_change_line && line.trim_end() == selected_line {
                    return Ok((unstaged_hunk, idx));
                }
            }
        }

        // Fallback to the passed-in hunk when no direct unstaged match is found.
        Ok((hunk.clone(), line_index))
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

        // Track exact staged change locations by (line_number, content_without_prefix)
        // This avoids false positives when the same text appears in multiple hunks.
        let mut staged_additions: HashSet<(usize, String)> = HashSet::new();
        let mut staged_deletions: HashSet<(usize, String)> = HashSet::new();

        diff.foreach(
            &mut |_, _| true,
            None,
            None,
            Some(&mut |_, _, line| {
                let content = String::from_utf8_lossy(line.content())
                    .trim_end_matches('\n')
                    .to_string();

                match line.origin() {
                    '+' => {
                        if let Some(new_lineno) = line.new_lineno() {
                            staged_additions.insert((new_lineno as usize, content));
                        }
                    }
                    '-' => {
                        if let Some(old_lineno) = line.old_lineno() {
                            staged_deletions.insert((old_lineno as usize, content));
                        }
                    }
                    _ => {}
                }
                true
            }),
        )?;

        // Walk the target hunk and compute exact old/new coordinates for each line,
        // then check whether that exact change exists in the staged index diff.
        let mut old_lineno = hunk.old_start;
        let mut new_lineno = hunk.new_start;

        for (hunk_idx, hunk_line) in hunk.lines.iter().enumerate() {
            if hunk_line.starts_with(' ') {
                old_lineno += 1;
                new_lineno += 1;
            } else if hunk_line.starts_with('-') && !hunk_line.starts_with("---") {
                let content = hunk_line[1..].trim_end_matches('\n').to_string();
                if staged_deletions.contains(&(old_lineno, content)) {
                    staged_lines.insert(hunk_idx);
                }
                old_lineno += 1;
            } else if hunk_line.starts_with('+') && !hunk_line.starts_with("+++") {
                let content = hunk_line[1..].trim_end_matches('\n').to_string();
                if staged_additions.contains(&(new_lineno, content)) {
                    staged_lines.insert(hunk_idx);
                }
                new_lineno += 1;
            }
        }
        
        Ok(staged_lines)
    }
    
    /// Stage a single line from a hunk
    pub fn stage_single_line(&self, hunk: &Hunk, line_index: usize, file_path: &Path) -> Result<()> {
        use std::process::Command;
        use std::io::Write;
        
        let (resolved_hunk, resolved_line_index) =
            self.resolve_line_against_unstaged_diff(hunk, line_index, file_path)?;

        let patch = self.build_single_line_patch(&resolved_hunk, resolved_line_index, file_path)?;
        
        // Try to apply the patch
        let mut child = Command::new("git")
            .arg("apply")
            .arg("--cached")
            .arg("--unidiff-zero")
            .arg("--recount")
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
        
        // Build the same single-line patch and apply it in reverse to index.
        let patch = self.build_single_line_patch(hunk, line_index, file_path)?;
        
        // Apply the reverse patch to the index using --cached and --reverse
        let mut child = Command::new("git")
            .arg("apply")
            .arg("--cached")
            .arg("--reverse")
            .arg("--unidiff-zero")
            .arg("--recount")
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
                .expect("failed to get system time")
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

        git_repo
            .unstage_single_line(hunk, line_index, Path::new("example.txt"))
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
}
