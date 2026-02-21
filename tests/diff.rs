use super::*;

#[test]
fn count_changes_pairs_adds_and_removes() {
    let file_path = PathBuf::from("src/main.rs");
    let hunk = Hunk::new(
        1,
        1,
        vec![
            "-old line\n".to_string(),
            "+new line\n".to_string(),
            "+extra line\n".to_string(),
        ],
        &file_path,
    );

    assert_eq!(hunk.count_changes(), 2);
}

#[test]
fn hunk_id_changes_when_content_changes() {
    let file_path = PathBuf::from("src/main.rs");
    let base = HunkId::new(&file_path, 10, 10, &["-a\n".to_string(), "+b\n".to_string()]);
    let changed =
        HunkId::new(&file_path, 10, 10, &["-a\n".to_string(), "+c\n".to_string()]);

    assert_ne!(base, changed);
}

#[test]
fn seen_tracker_marks_and_clears_hunks() {
    let file_path = PathBuf::from("src/lib.rs");
    let hunk_id = HunkId::new(&file_path, 3, 3, &["+line\n".to_string()]);
    let mut tracker = SeenTracker::new();

    assert!(!tracker.is_seen(&hunk_id));
    tracker.mark_seen(&hunk_id);
    assert!(tracker.is_seen(&hunk_id));

    tracker.remove_file_hunks(&file_path);
    assert!(!tracker.is_seen(&hunk_id));

    tracker.mark_seen(&hunk_id);
    tracker.clear();
    assert!(!tracker.is_seen(&hunk_id));
}

#[test]
fn hunk_format_and_constructor_defaults() {
    let file_path = PathBuf::from("src/main.rs");
    let lines = vec![" context\n".to_string(), "+added\n".to_string()];
    let hunk = Hunk::new(4, 7, lines.clone(), &file_path);

    assert_eq!(hunk.format(), lines.concat());
    assert!(!hunk.seen);
    assert!(!hunk.staged);
    assert!(hunk.staged_line_indices.is_empty());
}

#[test]
fn seen_tracker_default_is_empty() {
    let file_path = PathBuf::from("src/default.rs");
    let hunk_id = HunkId::new(&file_path, 1, 1, &["+x\n".to_string()]);
    let mut tracker = SeenTracker::default();
    assert!(!tracker.is_seen(&hunk_id));

    tracker.mark_seen(&hunk_id);
    assert!(tracker.is_seen(&hunk_id));
}
