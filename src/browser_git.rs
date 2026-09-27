//! Adapter from tui2web's simulated repository to Hunky's real UI/domain model.
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::Path,
    rc::Rc,
    time::UNIX_EPOCH,
};

use anyhow::{anyhow, Result};
use tui2web::{
    fs::{Filesystem, MemoryFilesystem},
    git::{GitRepository, InMemoryGitRepository, ReviewHunk, ReviewSource},
};

use crate::diff::{CommitInfo, DiffSnapshot, FileChange, Hunk, HunkId};

#[derive(Clone)]
pub struct GitRepo {
    pub repository: Rc<RefCell<InMemoryGitRepository>>,
    hunks: Rc<RefCell<HashMap<HunkId, ReviewHunk>>>,
}

impl GitRepo {
    pub fn fixture() -> Result<Self> {
        let mut fs = MemoryFilesystem::new();
        fs.create_dir_all("src")?;
        fs.write_file("README.md", b"# Orchard\n\nA tiny fruit shop.\n")?;
        fs.write_file("obsolete.txt", b"Remove this old inventory export.\n")?;
        fs.write_file(
            "src/shop.rs",
            include_bytes!("../browser-fixture/shop.head.rs"),
        )?;
        let mut repository = InMemoryGitRepository::new(fs);
        for path in repository.filesystem().list_files() {
            repository.stage_file(&path)?;
        }
        repository.commit("Initial fruit shop", "Demo author")?;
        repository
            .filesystem_mut()
            .write_file("README.md", b"# Orchard\n\nA tiny, friendly fruit shop.\n")?;
        repository.stage_file("README.md")?;
        repository.commit("Describe the shop", "Demo author")?;
        repository.filesystem_mut().write_file(
            "src/shop.rs",
            include_bytes!("../browser-fixture/shop.worktree.rs"),
        )?;
        repository.filesystem_mut().write_file(
            "README.md",
            b"# Orchard\n\nA tiny, friendly fruit shop.\n\nNow with seasonal specials.\n",
        )?;
        repository.stage_file("README.md")?;
        repository.filesystem_mut().write_file(
            "README.md",
            b"# Orchard\n\nA tiny, friendly fruit shop.\n\nNow with seasonal specials.\nBrowser edits stay local.\n",
        )?;
        repository.filesystem_mut().remove_file("obsolete.txt")?;
        repository.filesystem_mut().write_file(
            "notes.txt",
            b"Try staging just one line.\nThen edit a file without changing the index.\n",
        )?;
        Ok(Self {
            repository: Rc::new(RefCell::new(repository)),
            hunks: Rc::new(RefCell::new(HashMap::new())),
        })
    }

    pub fn get_diff_snapshot(&self) -> Result<DiffSnapshot> {
        let review = self.repository.borrow().review()?;
        let mut cache = self.hunks.borrow_mut();
        cache.clear();
        let files = review
            .into_iter()
            .map(|file| {
                let path = Path::new(&file.path).to_path_buf();
                let hunks = file
                    .hunks
                    .into_iter()
                    .map(|review| {
                        let mut hunk = Hunk::new(
                            review.old_start,
                            review.new_start,
                            review.lines.iter().map(|line| line.text.clone()).collect(),
                            &path,
                        );
                        hunk.id.index_only = review.source == ReviewSource::IndexOnly;
                        hunk.staged_line_indices = review
                            .lines
                            .iter()
                            .enumerate()
                            .filter_map(|(index, line)| {
                                (line.change.is_some() && line.staged).then_some(index)
                            })
                            .collect();
                        let changes = review
                            .lines
                            .iter()
                            .filter(|line| line.change.is_some())
                            .count();
                        hunk.staged = changes > 0 && changes == hunk.staged_line_indices.len();
                        cache.insert(hunk.id.clone(), review);
                        hunk
                    })
                    .collect();
                FileChange {
                    path,
                    status: file.status.to_string(),
                    hunks,
                }
            })
            .collect();
        Ok(DiffSnapshot {
            timestamp: UNIX_EPOCH,
            files,
        })
    }

    fn review_hunk(&self, hunk: &Hunk) -> Result<ReviewHunk> {
        self.hunks
            .borrow()
            .get(&hunk.id)
            .cloned()
            .ok_or_else(|| anyhow!("Selection is stale; refresh the diff"))
    }

    pub fn detect_staged_lines(&self, hunk: &Hunk, _path: &Path) -> Result<HashSet<usize>> {
        Ok(self
            .review_hunk(hunk)?
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| (line.staged && line.change.is_some()).then_some(index))
            .collect())
    }

    pub fn toggle_hunk_staging(&self, hunk: &Hunk, _path: &Path) -> Result<bool> {
        Ok(self
            .repository
            .borrow_mut()
            .toggle_hunk(&self.review_hunk(hunk)?)?)
    }

    pub fn stage_single_line(&self, hunk: &Hunk, index: usize, _path: &Path) -> Result<()> {
        let review = self.review_hunk(hunk)?;
        let change = review
            .lines
            .get(index)
            .and_then(|line| line.change.as_ref())
            .ok_or_else(|| anyhow!("Select an added or removed line"))?;
        self.repository.borrow_mut().stage_change(change)?;
        Ok(())
    }

    pub fn unstage_single_line(&self, hunk: &Hunk, index: usize, _path: &Path) -> Result<()> {
        let review = self.review_hunk(hunk)?;
        let change = review
            .lines
            .get(index)
            .and_then(|line| line.change.as_ref())
            .ok_or_else(|| anyhow!("Select an added or removed line"))?;
        self.repository.borrow_mut().unstage_change(change)?;
        Ok(())
    }

    pub fn stage_file(&self, path: &Path) -> Result<()> {
        self.repository
            .borrow_mut()
            .stage_file(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
        Ok(())
    }

    pub fn unstage_file(&self, path: &Path) -> Result<()> {
        self.repository
            .borrow_mut()
            .unstage_file(path.to_str().ok_or_else(|| anyhow!("Invalid path"))?)?;
        Ok(())
    }

    pub fn get_recent_commits(&self, count: usize) -> Result<Vec<CommitInfo>> {
        Ok(self
            .repository
            .borrow()
            .log(count)?
            .into_iter()
            .map(|commit| CommitInfo {
                sha: commit.sha,
                short_sha: commit.short_sha,
                summary: commit.summary,
                author: commit.author,
            })
            .collect())
    }

    pub fn get_commit_diff(&self, sha: &str) -> Result<DiffSnapshot> {
        let files = self
            .repository
            .borrow()
            .diff_commit(sha)?
            .into_iter()
            .map(|file| {
                let path = Path::new(&file.path).to_path_buf();
                FileChange {
                    hunks: file
                        .hunks
                        .into_iter()
                        .map(|hunk| Hunk::new(hunk.old_start, hunk.new_start, hunk.lines, &path))
                        .collect(),
                    path,
                    status: file.status.to_string(),
                }
            })
            .collect();
        Ok(DiffSnapshot {
            timestamp: UNIX_EPOCH,
            files,
        })
    }
}
