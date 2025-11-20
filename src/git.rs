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
        
        // Get the diff between HEAD and working directory
        let mut diff_opts = DiffOptions::new();
        diff_opts.include_untracked(true);
        diff_opts.recurse_untracked_dirs(true);
        
        let diff = repo.diff_index_to_workdir(None, Some(&mut diff_opts))?;
        
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
        
        let diff = repo.diff_index_to_workdir(None, Some(&mut diff_opts))?;
        
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
    
    pub fn get_status(&self) -> Result<String> {
        let repo = Repository::open(&self.repo_path)?;
        let statuses = repo.statuses(None)?;
        
        let mut status_lines = Vec::new();
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                let status = entry.status();
                let status_str = if status.is_wt_new() {
                    "new file"
                } else if status.is_wt_modified() {
                    "modified"
                } else if status.is_wt_deleted() {
                    "deleted"
                } else if status.is_wt_renamed() {
                    "renamed"
                } else {
                    "unknown"
                };
                status_lines.push(format!("{}: {}", status_str, path));
            }
        }
        
        Ok(status_lines.join("\n"))
    }
}
