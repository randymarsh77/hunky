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
        
        let mut hunks = Vec::new();
        let path_buf = path.to_path_buf();
        
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = String::from_utf8_lossy(line.content()).to_string();
            let line_type = match line.origin() {
                '+' => "addition",
                '-' => "deletion",
                ' ' => "context",
                _ => "other",
            };
            
            // Group consecutive lines into hunks
            if hunks.is_empty() || line_type != "context" {
                let lines = vec![format!("{}{}", line.origin(), content)];
                let old_start = line.old_lineno().unwrap_or(0) as usize;
                let new_start = line.new_lineno().unwrap_or(0) as usize;
                hunks.push(Hunk::new(old_start, new_start, lines, &path_buf));
            } else {
                if let Some(last_hunk) = hunks.last_mut() {
                    last_hunk.lines.push(format!("{}{}", line.origin(), content));
                }
            }
            
            true
        })?;
        
        Ok(hunks)
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
