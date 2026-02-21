use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository};
use std::path::{Path, PathBuf};

use crate::diff::{DiffSnapshot, FileChange, Hunk};

#[derive(Clone)]
pub struct GitRepo {
    repo_path: PathBuf,
}

impl GitRepo {
    fn is_change_line(line: &str) -> bool {
        (line.starts_with('+') && !line.starts_with("+++"))
            || (line.starts_with('-') && !line.starts_with("---"))
    }

    fn hunk_line_coordinates(
        hunk: &Hunk,
        line_index: usize,
    ) -> Option<(Option<usize>, Option<usize>)> {
        if line_index >= hunk.lines.len() {
            return None;
        }

        let mut old_lineno = hunk.old_start;
        let mut new_lineno = hunk.new_start;

        for (idx, line) in hunk.lines.iter().enumerate() {
            let coords = if line.starts_with(' ') {
                let current = (Some(old_lineno), Some(new_lineno));
                old_lineno += 1;
                new_lineno += 1;
                current
            } else if line.starts_with('-') && !line.starts_with("---") {
                let current = (Some(old_lineno), None);
                old_lineno += 1;
                current
            } else if line.starts_with('+') && !line.starts_with("+++") {
                let current = (None, Some(new_lineno));
                new_lineno += 1;
                current
            } else {
                continue;
            };

            if idx == line_index {
                return Some(coords);
            }
        }

        None
    }

    fn single_line_patch_header(
        hunk: &Hunk,
        line_index: usize,
    ) -> Option<(usize, usize, usize, usize)> {
        if line_index >= hunk.lines.len() {
            return None;
        }

        let mut old_lineno = hunk.old_start;
        let mut new_lineno = hunk.new_start;

        for (idx, line) in hunk.lines.iter().enumerate() {
            if idx == line_index {
                if line.starts_with('-') && !line.starts_with("---") {
                    return Some((old_lineno, 1, new_lineno, 0));
                }
                if line.starts_with('+') && !line.starts_with("+++") {
                    return Some((old_lineno, 0, new_lineno, 1));
                }
                return None;
            }

            if line.starts_with(' ') {
                old_lineno += 1;
                new_lineno += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                old_lineno += 1;
            } else if line.starts_with('+') && !line.starts_with("+++") {
                new_lineno += 1;
            }
        }

        None
    }

    fn build_single_line_patch(
        &self,
        hunk: &Hunk,
        line_index: usize,
        file_path: &Path,
    ) -> Result<String> {
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

        let (old_start, old_line_count, new_start, new_line_count) =
            Self::single_line_patch_header(hunk, line_index)
                .ok_or_else(|| anyhow::anyhow!("Can only patch + or - lines"))?;

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

        patch.push_str(selected_line);
        if !selected_line.ends_with('\n') {
            patch.push('\n');
        }

        Ok(patch)
    }

    fn apply_single_line_patch_raw(
        &self,
        hunk: &Hunk,
        line_index: usize,
        file_path: &Path,
        reverse: bool,
    ) -> Result<()> {
        use std::io::Write;
        use std::process::Command;

        let patch = self.build_single_line_patch(hunk, line_index, file_path)?;

        let mut cmd = Command::new("git");
        cmd.arg("apply")
            .arg("--cached")
            .arg("--unidiff-zero")
            .arg("--recount");
        if reverse {
            cmd.arg("--reverse");
        }
        cmd.arg("-")
            .current_dir(&self.repo_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

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
            let action = if reverse { "unstage" } else { "stage" };
            return Err(anyhow::anyhow!(
                "Failed to {} line: {}\nPatch was:\n{}",
                action,
                error_msg,
                patch_preview
            ));
        }

        Ok(())
    }

    fn is_noop_patch_apply_error(err: &anyhow::Error) -> bool {
        let msg = err.to_string().to_lowercase();
        msg.contains("patch does not apply") || msg.contains("no valid patches in input")
    }

    fn change_line_indices(hunk: &Hunk) -> std::collections::HashSet<usize> {
        hunk.lines
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| Self::is_change_line(line).then_some(idx))
            .collect()
    }

    fn sort_indices_desc_by_position(
        hunk: &Hunk,
        indices: &std::collections::HashSet<usize>,
    ) -> Vec<usize> {
        let mut ordered: Vec<usize> = indices.iter().copied().collect();
        ordered.sort_by(|a, b| {
            let a_pos = Self::hunk_line_coordinates(hunk, *a)
                .map(|(old, new)| old.or(new).unwrap_or(0))
                .unwrap_or(0);
            let b_pos = Self::hunk_line_coordinates(hunk, *b)
                .map(|(old, new)| old.or(new).unwrap_or(0))
                .unwrap_or(0);
            b_pos.cmp(&a_pos).then_with(|| b.cmp(a))
        });
        ordered
    }

    fn unstaged_hunk_count_for_file(&self, file_path: &Path) -> Result<usize> {
        let repo = Repository::open(&self.repo_path)?;
        let index = repo.index()?;

        let mut diff_opts = DiffOptions::new();
        diff_opts.pathspec(file_path);
        diff_opts.context_lines(3);

        let diff = repo.diff_index_to_workdir(Some(&index), Some(&mut diff_opts))?;
        let mut count: usize = 0;
        diff.foreach(
            &mut |_, _| true,
            None,
            Some(&mut |_, _| {
                count += 1;
                true
            }),
            None,
        )?;

        Ok(count)
    }

    fn set_hunk_staged_lines_with_reset(
        &self,
        hunk: &Hunk,
        file_path: &Path,
        desired_staged_indices: &std::collections::HashSet<usize>,
        reset_indices: &std::collections::HashSet<usize>,
    ) -> Result<()> {
        use std::collections::HashSet;

        let all_change_indices = Self::change_line_indices(hunk);
        let desired: HashSet<usize> = desired_staged_indices
            .intersection(&all_change_indices)
            .copied()
            .collect();

        let reset_filtered: HashSet<usize> = reset_indices
            .intersection(&all_change_indices)
            .copied()
            .collect();

        crate::logger::debug(format!(
            "set_hunk_staged_lines_with_reset file={} old_start={} new_start={} all_changes={} desired={} reset={}",
            file_path.display(),
            hunk.old_start,
            hunk.new_start,
            all_change_indices.len(),
            desired.len(),
            reset_indices.len()
        ));

        // Fast path for whole-hunk unstage requests.
        if desired.is_empty() && reset_filtered.len() == all_change_indices.len() {
            if let Err(full_unstage_err) = self.unstage_hunk(hunk, file_path) {
                crate::logger::debug(format!(
                    "full hunk unstage failed; aborting without fallback file={}: {}",
                    file_path.display(),
                    full_unstage_err
                ));
                return Err(anyhow::anyhow!(
                    "Failed to unstage hunk in {}",
                    file_path.display()
                ));
            } else {
                return Ok(());
            }
        }

        // Reset staged state for requested lines and tolerate no-op reverse-patch failures.
        let reset_order = Self::sort_indices_desc_by_position(hunk, &reset_filtered);
        for idx in reset_order {
            if let Err(e) = self.apply_single_line_patch_raw(hunk, idx, file_path, true) {
                if !Self::is_noop_patch_apply_error(&e) {
                    crate::logger::warn(format!(
                        "reset reverse-apply failed at idx={} file={}: {}",
                        idx,
                        file_path.display(),
                        e
                    ));
                    return Err(e);
                }
                crate::logger::trace(format!(
                    "reset reverse-apply no-op at idx={} file={}",
                    idx,
                    file_path.display()
                ));
            }
        }

        if desired.is_empty() {
            return Ok(());
        }

        if desired.len() == all_change_indices.len() {
            // Prefer atomic hunk stage; avoid per-line fallback for full-hunk requests
            // because it can drift/duplicate in partial-index states.
            if let Err(stage_err) = self.stage_hunk(hunk, file_path) {
                crate::logger::debug(format!(
                    "full hunk stage failed in set_hunk_staged_lines_with_reset; aborting without fallback file={}: {}",
                    file_path.display(),
                    stage_err
                ));
                return Err(anyhow::anyhow!(
                    "Failed to fully stage hunk in {}",
                    file_path.display()
                ));
            }

            return Ok(());
        }

        let stage_order = Self::sort_indices_desc_by_position(hunk, &desired);
        for idx in stage_order {
            self.apply_single_line_patch_raw(hunk, idx, file_path, false)?;
        }

        Ok(())
    }

    fn set_hunk_staged_lines(
        &self,
        hunk: &Hunk,
        file_path: &Path,
        desired_staged_indices: &std::collections::HashSet<usize>,
    ) -> Result<()> {
        let currently_staged = self.detect_staged_lines(hunk, file_path)?;
        self.set_hunk_staged_lines_with_reset(
            hunk,
            file_path,
            desired_staged_indices,
            &currently_staged,
        )
    }

    pub fn toggle_hunk_staging(&self, hunk: &Hunk, file_path: &Path) -> Result<bool> {
        // Returns true if final state is staged, false if final state is unstaged.
        let currently_staged = self.detect_staged_lines(hunk, file_path)?;
        let all_change_indices = Self::change_line_indices(hunk);

        // Hunk-mode `s` behavior:
        // - fully staged hunk => unstage hunk
        // - partially/unstaged hunk => stage remaining lines
        if currently_staged.len() < all_change_indices.len() {
            // Partial hunk: safely stage remaining unstaged changes only when this
            // file has a single unstaged hunk (equivalent to "stage the rest").
            if self.unstaged_hunk_count_for_file(file_path)? == 1 {
                self.stage_file(file_path)?;
                return Ok(true);
            }

            // Multiple unstaged hunks in file: best-effort stage only the remaining
            // lines in the selected hunk. Use both detected staged lines and UI hints
            // to avoid reapplying already-staged lines.
            let mut staged_known = currently_staged.clone();
            for idx in &hunk.staged_line_indices {
                if all_change_indices.contains(idx) {
                    staged_known.insert(*idx);
                }
            }

            let mut remaining: std::collections::HashSet<usize> = all_change_indices
                .difference(&staged_known)
                .copied()
                .collect();

            // Run up to two passes to absorb coordinate drift after first apply set.
            for _ in 0..2 {
                if remaining.is_empty() {
                    return Ok(true);
                }

                let stage_order = Self::sort_indices_desc_by_position(hunk, &remaining);
                let mut progress = false;
                for idx in stage_order {
                    match self.apply_single_line_patch_raw(hunk, idx, file_path, false) {
                        Ok(_) => progress = true,
                        Err(e) if Self::is_noop_patch_apply_error(&e) => {
                            crate::logger::trace(format!(
                                "toggle stage remaining no-op at idx={} file={}",
                                idx,
                                file_path.display()
                            ));
                        }
                        Err(e) => return Err(e),
                    }
                }

                let staged_after = self.detect_staged_lines(hunk, file_path)?;
                remaining = all_change_indices
                    .difference(&staged_after)
                    .copied()
                    .collect();

                if !progress {
                    break;
                }
            }

            return Err(anyhow::anyhow!(
                "Cannot safely complete partial hunk staging in {} (multiple unstaged hunks)",
                file_path.display()
            ));
        }

        // Fully staged hunk => unstage full hunk.
        match self.unstage_hunk(hunk, file_path) {
            Ok(_) => Ok(false),
            Err(e) => {
                crate::logger::debug(format!(
                    "full hunk unstage failed in toggle; aborting without fallback file={}: {}",
                    file_path.display(),
                    e
                ));
                Err(anyhow::anyhow!(
                    "Failed to unstage hunk in {}",
                    file_path.display()
                ))
            }
        }
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

    /// Run `git commit` interactively, allowing Git to launch the configured editor.
    pub fn commit_with_editor(&self) -> Result<std::process::ExitStatus> {
        use std::process::Command;

        let status = Command::new("git")
            .arg("commit")
            .current_dir(&self.repo_path)
            .status()
            .context("Failed to run `git commit`")?;

        Ok(status)
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
        let diff =
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

        let mut files = Vec::new();

        diff.foreach(
            &mut |delta, _progress| {
                let file_path = match delta.status() {
                    Delta::Added | Delta::Modified | Delta::Deleted => {
                        delta.new_file().path().or_else(|| delta.old_file().path())
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
        let diff =
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

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
                        &path_clone,
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
                    lines_clone2
                        .borrow_mut()
                        .push(format!("{}{}", line.origin(), content));
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
                &path_buf,
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
        use std::io::Write;
        use std::process::Command;

        // Create a proper unified diff patch
        let mut patch = String::new();

        // Diff header
        patch.push_str(&format!(
            "diff --git a/{} b/{}\n",
            file_path.display(),
            file_path.display()
        ));
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
        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, old_lines, hunk.new_start, new_lines
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
    pub fn detect_staged_lines(
        &self,
        hunk: &Hunk,
        file_path: &Path,
    ) -> Result<std::collections::HashSet<usize>> {
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

        let index = repo.index()?;
        let mut unstaged_opts = DiffOptions::new();
        unstaged_opts.pathspec(file_path);
        let unstaged_diff = repo.diff_index_to_workdir(Some(&index), Some(&mut unstaged_opts))?;

        let mut staged_lines = HashSet::new();

        // Track exact staged deletions by (HEAD old line number, content_without_prefix).
        // For additions we prefer deriving from unstaged additions (index->worktree) because
        // HEAD->index new line numbers can diverge from HEAD->worktree when unstaged changes
        // exist earlier in the same hunk.
        let mut staged_deletions: HashSet<(usize, String)> = HashSet::new();
        let mut unstaged_additions: HashSet<(usize, String)> = HashSet::new();

        diff.foreach(
            &mut |_, _| true,
            None,
            None,
            Some(&mut |_, _, line| {
                let content = String::from_utf8_lossy(line.content())
                    .trim_end_matches('\n')
                    .to_string();

                match line.origin() {
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

        unstaged_diff.foreach(
            &mut |_, _| true,
            None,
            None,
            Some(&mut |_, _, line| {
                if line.origin() == '+' {
                    let content = String::from_utf8_lossy(line.content())
                        .trim_end_matches('\n')
                        .to_string();
                    if let Some(new_lineno) = line.new_lineno() {
                        unstaged_additions.insert((new_lineno as usize, content));
                    }
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
                // '+' line is staged if it is NOT present as an unstaged worktree addition.
                if !unstaged_additions.contains(&(new_lineno, content)) {
                    staged_lines.insert(hunk_idx);
                }
                new_lineno += 1;
            }
        }

        Ok(staged_lines)
    }

    /// Stage a single line from a hunk
    pub fn stage_single_line(
        &self,
        hunk: &Hunk,
        line_index: usize,
        file_path: &Path,
    ) -> Result<()> {
        let selected_line = hunk
            .lines
            .get(line_index)
            .ok_or_else(|| anyhow::anyhow!("Line index out of bounds"))?;
        if !Self::is_change_line(selected_line) {
            return Err(anyhow::anyhow!("Can only stage + or - lines"));
        }

        let currently_staged = self.detect_staged_lines(hunk, file_path)?;
        if currently_staged.contains(&line_index) {
            crate::logger::trace(format!(
                "stage_single_line no-op already staged file={} line_index={}",
                file_path.display(),
                line_index
            ));
            return Ok(());
        }

        crate::logger::debug(format!(
            "stage_single_line file={} line_index={}",
            file_path.display(),
            line_index
        ));

        self.apply_single_line_patch_raw(hunk, line_index, file_path, false)?;

        let staged_after = self.detect_staged_lines(hunk, file_path)?;
        if !staged_after.contains(&line_index) {
            return Err(anyhow::anyhow!(
                "Failed to stage selected line {} in {}",
                line_index,
                file_path.display()
            ));
        }

        Ok(())
    }

    /// Unstage a single line from a hunk
    pub fn unstage_single_line(
        &self,
        hunk: &Hunk,
        line_index: usize,
        file_path: &Path,
    ) -> Result<()> {
        let selected_line = hunk
            .lines
            .get(line_index)
            .ok_or_else(|| anyhow::anyhow!("Line index out of bounds"))?;
        if !Self::is_change_line(selected_line) {
            return Err(anyhow::anyhow!("Can only unstage + or - lines"));
        }

        let currently_staged = self.detect_staged_lines(hunk, file_path)?;
        if !currently_staged.contains(&line_index) {
            crate::logger::trace(format!(
                "unstage_single_line no-op already unstaged file={} line_index={}",
                file_path.display(),
                line_index
            ));
            return Ok(());
        }

        crate::logger::debug(format!(
            "unstage_single_line file={} line_index={}",
            file_path.display(),
            line_index
        ));

        self.apply_single_line_patch_raw(hunk, line_index, file_path, true)?;

        let staged_after = self.detect_staged_lines(hunk, file_path)?;
        if staged_after.contains(&line_index) {
            return Err(anyhow::anyhow!(
                "Failed to unstage selected line {} in {}",
                line_index,
                file_path.display()
            ));
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
        use std::io::Write;
        use std::process::Command;

        // Create a proper unified diff patch
        let mut patch = String::new();

        // Diff header
        patch.push_str(&format!(
            "diff --git a/{} b/{}\n",
            file_path.display(),
            file_path.display()
        ));
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
        patch.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, old_lines, hunk.new_start, new_lines
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
#[path = "../tests/git.rs"]
mod tests;
