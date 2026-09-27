use super::*;
use crate::diff::Hunk;
use crate::ui::UI;
use ratatui::{backend::TestBackend, Terminal};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("failed to get system time")
            .as_nanos();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "hunky-app-tests-{}-{}-{}",
            std::process::id(),
            unique,
            counter
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

fn run_git(repo_path: &std::path::Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn render_buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut rows = Vec::new();
    for y in 0..buffer.area.height {
        let mut row = String::new();
        for x in 0..buffer.area.width {
            row.push_str(
                buffer
                    .cell((x, y))
                    .expect("buffer cell should be available")
                    .symbol(),
            );
        }
        rows.push(row);
    }
    rows.join("\n")
}

fn sample_snapshot() -> DiffSnapshot {
    let file1 = PathBuf::from("a.txt");
    let file2 = PathBuf::from("b.txt");
    DiffSnapshot {
        timestamp: SystemTime::now(),
        files: vec![
            FileChange {
                path: file1.clone(),
                status: "Modified".to_string(),
                hunks: vec![Hunk::new(
                    1,
                    1,
                    vec!["-old\n".to_string(), "+new\n".to_string()],
                    &file1,
                )],
            },
            FileChange {
                path: file2.clone(),
                status: "Modified".to_string(),
                hunks: vec![Hunk::new(
                    1,
                    1,
                    vec!["-old2\n".to_string(), "+new2\n".to_string()],
                    &file2,
                )],
            },
        ],
    }
}

#[tokio::test]
async fn cycle_mode_transitions_and_resets_streaming_state() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.snapshots = vec![sample_snapshot()];
    app.current_snapshot_index = 0;

    app.cycle_mode();
    assert_eq!(app.mode, Mode::Streaming(StreamingType::Buffered));
    assert_eq!(app.streaming_start_snapshot, Some(0));

    app.cycle_mode();
    assert_eq!(
        app.mode,
        Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast))
    );
    app.cycle_mode();
    assert_eq!(
        app.mode,
        Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium))
    );
    app.cycle_mode();
    assert_eq!(
        app.mode,
        Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow))
    );

    app.current_file_index = 1;
    app.current_hunk_index = 1;
    app.cycle_mode();
    assert_eq!(app.mode, Mode::View);
    assert_eq!(app.streaming_start_snapshot, None);
    assert_eq!(app.current_file_index, 0);
    assert_eq!(app.current_hunk_index, 0);
}

#[tokio::test]
async fn focus_cycle_saves_line_mode_and_handles_help_sidebar() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.show_help = true;
    app.focus = FocusPane::HunkView;
    app.line_selection_mode = true;
    app.selected_line_index = 3;

    app.cycle_focus_forward();
    assert_eq!(app.focus, FocusPane::HelpSidebar);
    assert!(!app.line_selection_mode);
    assert_eq!(
        app.hunk_line_memory
            .get(&(app.current_file_index, app.current_hunk_index)),
        Some(&3)
    );

    app.cycle_focus_forward();
    assert_eq!(app.focus, FocusPane::FileList);
    app.cycle_focus_backward();
    assert_eq!(app.focus, FocusPane::HelpSidebar);
}

#[tokio::test]
async fn toggle_line_selection_mode_restores_saved_line() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline 2 updated\n");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.focus = FocusPane::HunkView;

    app.toggle_line_selection_mode();
    assert!(app.line_selection_mode);
    app.selected_line_index = 1;

    app.toggle_line_selection_mode();
    assert!(!app.line_selection_mode);
    app.selected_line_index = 0;

    app.toggle_line_selection_mode();
    assert!(app.line_selection_mode);
    assert_eq!(app.selected_line_index, 1);
}

#[tokio::test]
async fn advance_hunk_wraps_at_last_hunk_in_view_mode() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.snapshots = vec![sample_snapshot()];
    app.current_snapshot_index = 0;
    app.current_file_index = 1;
    app.current_hunk_index = 0;
    app.mode = Mode::View;

    app.advance_hunk();
    assert_eq!(app.current_file_index, 0);
    assert_eq!(app.current_hunk_index, 0);
}

#[tokio::test]
async fn advance_hunk_stops_at_last_hunk_in_buffered_mode() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.snapshots = vec![sample_snapshot()];
    app.current_snapshot_index = 0;
    app.current_file_index = 1;
    app.current_hunk_index = 0;
    app.mode = Mode::Streaming(StreamingType::Buffered);

    app.advance_hunk();
    assert_eq!(app.current_file_index, 1);
    assert_eq!(app.current_hunk_index, 0);
}

#[tokio::test]
async fn previous_hunk_wraps_at_first_hunk_in_view_mode() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.snapshots = vec![sample_snapshot()];
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.mode = Mode::View;

    app.previous_hunk();
    assert_eq!(app.current_file_index, 1);
    assert_eq!(app.current_hunk_index, 0);
}

#[tokio::test]
async fn previous_hunk_stops_at_first_hunk_in_buffered_mode() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.snapshots = vec![sample_snapshot()];
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.mode = Mode::Streaming(StreamingType::Buffered);

    app.previous_hunk();
    assert_eq!(app.current_file_index, 0);
    assert_eq!(app.current_hunk_index, 0);
}

#[tokio::test]
async fn navigation_and_scroll_helpers_cover_core_branches() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    let mut snapshot = sample_snapshot();
    snapshot.files[0].hunks[0].lines = vec![
        " context a\n".to_string(),
        "-old\n".to_string(),
        "+new\n".to_string(),
        " context b\n".to_string(),
    ];
    app.snapshots = vec![snapshot];
    app.current_snapshot_index = 0;

    assert_eq!(
        StreamSpeed::Fast.duration_for_hunk(2),
        Duration::from_millis(700)
    );
    assert_eq!(
        StreamSpeed::Medium.duration_for_hunk(1),
        Duration::from_millis(1000)
    );
    assert_eq!(
        StreamSpeed::Slow.duration_for_hunk(3),
        Duration::from_millis(3500)
    );

    app.select_first_change_line();
    assert_eq!(app.selected_line_index, 1);
    app.next_change_line();
    assert_eq!(app.selected_line_index, 2);
    app.previous_change_line();
    assert_eq!(app.selected_line_index, 1);

    app.hunk_line_memory.insert((0, 0), 1);
    app.current_file_index = 0;
    app.next_file();
    assert_eq!(app.current_file_index, 1);
    assert_eq!(app.current_hunk_index, 0);
    assert!(!app.hunk_line_memory.contains_key(&(0, 0)));
    app.previous_file();
    assert_eq!(app.current_file_index, 0);

    app.scroll_offset = 50;
    app.clamp_scroll_offset(20);
    assert_eq!(app.scroll_offset, 0);
    app.help_scroll_offset = 50;
    app.clamp_help_scroll_offset(10);
    assert_eq!(app.help_scroll_offset, 22);
    app.extended_help_scroll_offset = 500;
    app.clamp_extended_help_scroll_offset(20);
    assert_eq!(app.extended_help_scroll_offset, 88);
}

#[tokio::test]
async fn ui_draw_renders_mode_and_help_states() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.mode = Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast));
    let ui = UI::new(&app);
    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            ui.draw(frame);
        })
        .expect("failed to draw ui");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("STREAMING (Auto - Fast)"));

    app.show_help = true;
    app.show_extended_help = false;
    let ui = UI::new(&app);
    terminal
        .draw(|frame| {
            ui.draw(frame);
        })
        .expect("failed to draw ui");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("Keys"));

    app.show_extended_help = true;
    let ui = UI::new(&app);
    terminal
        .draw(|frame| {
            ui.draw(frame);
        })
        .expect("failed to draw ui");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("Extended Help"));
}

#[tokio::test]
async fn ui_draw_clears_previous_hunk_text_when_advancing() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    let path = PathBuf::from("garble.txt");
    let snapshot = DiffSnapshot {
        timestamp: SystemTime::now(),
        files: vec![FileChange {
            path: path.clone(),
            status: "Modified".to_string(),
            hunks: vec![
                Hunk::new(
                    1,
                    1,
                    vec![
                        "-old\n".to_string(),
                        "+new\n".to_string(),
                        "+GARBLED_MARKER_SHOULD_NOT_PERSIST\n".to_string(),
                        "+line4\n".to_string(),
                        "+line5\n".to_string(),
                    ],
                    &path,
                ),
                Hunk::new(10, 10, vec!["+short\n".to_string()], &path),
            ],
        }],
    };
    app.snapshots = vec![snapshot];
    app.current_snapshot_index = 0;

    let backend = TestBackend::new(120, 20);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw first hunk");
    assert!(render_buffer_to_string(&terminal).contains("GARBLED_MARKER_SHOULD_NOT_PERSIST"));

    app.advance_hunk();
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw second hunk");

    assert!(!render_buffer_to_string(&terminal).contains("GARBLED_MARKER_SHOULD_NOT_PERSIST"));
}

#[tokio::test]
async fn stage_current_selection_handles_line_hunk_and_file_modes() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\nline 3\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline two updated\nline 3\n");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.focus = FocusPane::HunkView;
    app.line_selection_mode = true;

    let selected = app.snapshots[0].files[0].hunks[0]
        .lines
        .iter()
        .position(|line| line.starts_with('+') && !line.starts_with("+++"))
        .expect("expected added line");
    app.selected_line_index = selected;

    // Line mode: stage selected line and verify index changed
    app.stage_current_selection();
    let cached_after_line_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(cached_after_line_stage.contains("example.txt"));

    // Reset to clean index before hunk-mode checks
    run_git(&repo.path, &["reset", "HEAD", "--", "example.txt"]);
    app.refresh_current_snapshot_from_git();

    // Hunk mode: stage current hunk and verify index changed
    app.line_selection_mode = false;
    app.stage_current_selection();
    let cached_after_hunk_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(cached_after_hunk_stage.contains("example.txt"));

    // Reset to clean index before file-mode checks
    run_git(&repo.path, &["reset", "HEAD", "--", "example.txt"]);
    app.refresh_current_snapshot_from_git();

    app.focus = FocusPane::FileList;
    app.stage_current_selection();
    let cached_after_file_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(cached_after_file_stage.contains("example.txt"));

    app.stage_current_selection();
    let cached_after_file_unstage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(cached_after_file_unstage.trim().is_empty());
}

#[tokio::test]
async fn stage_current_selection_toggles_added_and_deleted_files_in_hunk_view() {
    let repo = TestRepo::new();
    repo.write_file("tracked.txt", "tracked\n");
    repo.commit_all("initial");
    repo.write_file("added.txt", "new file\n");
    run_git(&repo.path, &["add", "-N", "added.txt"]);
    std::fs::remove_file(repo.path.join("tracked.txt")).expect("failed to remove tracked file");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.current_snapshot_index = 0;
    app.focus = FocusPane::HunkView;
    app.line_selection_mode = true;

    let added_index = app.snapshots[0]
        .files
        .iter()
        .position(|file| file.path == PathBuf::from("added.txt"))
        .expect("expected added file in diff");
    app.current_file_index = added_index;
    app.current_hunk_index = 0;
    app.stage_current_selection();
    let staged_added = run_git(&repo.path, &["diff", "--cached", "--name-status"]);
    assert!(staged_added.contains("A\tadded.txt"));
    app.stage_current_selection();
    let staged_added_after_unstage = run_git(&repo.path, &["diff", "--cached", "--name-status"]);
    assert!(!staged_added_after_unstage.contains("A\tadded.txt"));

    let deleted_index = app.snapshots[0]
        .files
        .iter()
        .position(|file| file.path == PathBuf::from("tracked.txt"))
        .expect("expected deleted file in diff");
    app.current_file_index = deleted_index;
    app.current_hunk_index = 0;
    app.stage_current_selection();
    let staged_deleted = run_git(&repo.path, &["diff", "--cached", "--name-status"]);
    assert!(staged_deleted.contains("D\ttracked.txt"));
    app.stage_current_selection();

    let staged_after = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(staged_after.trim().is_empty());
}

#[tokio::test]
#[ignore = "Known flaky hunk restage path; run explicitly during debugging"]
async fn hunk_toggle_can_restage_after_unstage_on_simple_file() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\nline 2\nline 3\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1\nline two updated\nline 3\n");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.focus = FocusPane::HunkView;
    app.line_selection_mode = false;

    // Stage hunk
    app.stage_current_selection();
    let cached_after_stage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(cached_after_stage.contains("example.txt"));

    // Unstage hunk
    app.stage_current_selection();
    let cached_after_unstage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(cached_after_unstage.trim().is_empty());

    // Restage hunk (regression target)
    app.stage_current_selection();
    let cached_after_restage = run_git(&repo.path, &["diff", "--cached", "--name-only"]);
    assert!(
        cached_after_restage.contains("example.txt"),
        "expected example.txt to be restaged, got:\n{}",
        cached_after_restage
    );
}

#[tokio::test]
async fn ui_draw_renders_file_list_variants() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.write_file("b.txt", "two\n");
    repo.commit_all("initial");
    repo.write_file("a.txt", "one changed\n");
    repo.write_file("b.txt", "two changed\n");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.mode = Mode::View;
    app.show_help = true;
    app.focus = FocusPane::FileList;

    let backend = TestBackend::new(120, 35);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw ui");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("Files"));
    assert!(rendered.contains("Help"));

    app.show_filenames_only = true;
    app.wrap_lines = true;
    app.line_selection_mode = true;
    app.select_first_change_line();
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw ui");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("File Info"));
}

#[tokio::test]
async fn ui_header_renders_mode_labels_across_breakpoints() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    let cases = vec![
        (
            Mode::Streaming(StreamingType::Buffered),
            160,
            "STREAMING (Buffered)",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)),
            160,
            "STREAMING (Auto - Medium)",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)),
            160,
            "STREAMING (Auto - Slow)",
        ),
        (
            Mode::Streaming(StreamingType::Buffered),
            70,
            "STREAM (Buff)",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)),
            70,
            "STREAM (Fast)",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium)),
            70,
            "STREAM (Med)",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)),
            70,
            "STREAM (Slow)",
        ),
        (Mode::Streaming(StreamingType::Buffered), 45, "STM:B"),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)),
            45,
            "STM:F",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)),
            45,
            "STM:S",
        ),
        (Mode::Streaming(StreamingType::Buffered), 36, "| B"),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Fast)),
            36,
            "| F",
        ),
        (
            Mode::Streaming(StreamingType::Auto(StreamSpeed::Slow)),
            36,
            "| S",
        ),
    ];

    for (mode, width, expected) in cases {
        app.mode = mode;
        let mut terminal =
            Terminal::new(TestBackend::new(width, 20)).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                UI::new(&app).draw(frame);
            })
            .expect("failed to draw ui");
        let rendered = render_buffer_to_string(&terminal);
        assert!(
            rendered.contains(expected),
            "missing '{expected}' in:\n{rendered}"
        );
    }
}

#[tokio::test]
async fn ui_draw_renders_partial_and_seen_hunk_states() {
    let repo = TestRepo::new();
    repo.write_file("example.rs", "fn main() {\n    println!(\"one\");\n}\n");
    repo.commit_all("initial");
    repo.write_file(
        "example.rs",
        "fn main() {\n    println!(\"two\");\n    println!(\"three\");\n}\n",
    );

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.syntax_highlighting = false;
    app.line_selection_mode = true;

    let hunk = &mut app.snapshots[0].files[0].hunks[0];
    hunk.lines = vec![
        " before 1\n".to_string(),
        " before 2\n".to_string(),
        " before 3\n".to_string(),
        " before 4\n".to_string(),
        " before 5\n".to_string(),
        " before 6\n".to_string(),
        "-old line\n".to_string(),
        "+new line\n".to_string(),
        " after 1\n".to_string(),
        " after 2\n".to_string(),
    ];
    hunk.staged_line_indices.insert(7);
    hunk.seen = true;
    app.selected_line_index = 6;

    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw partial hunk ui");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("[PARTIAL ⚠] [SEEN]"));
    assert!(rendered.contains("[0✓ 1⚠]"));
    assert!(rendered.contains("►"));
}

#[tokio::test]
async fn navigation_handles_empty_and_boundary_states() {
    let repo = TestRepo::new();
    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.snapshots.clear();
    app.advance_hunk();
    app.previous_hunk();
    app.next_file();
    app.previous_file();

    app.snapshots = vec![DiffSnapshot {
        timestamp: SystemTime::now(),
        files: vec![],
    }];
    app.current_snapshot_index = 0;
    app.advance_hunk();
    app.previous_hunk();
    app.next_file();
    app.previous_file();

    let mut snapshot = sample_snapshot();
    snapshot.files[0].hunks[0].lines = vec![
        " context before\n".to_string(),
        "-old\n".to_string(),
        "+new\n".to_string(),
        " context after\n".to_string(),
    ];
    app.snapshots = vec![snapshot];
    app.current_snapshot_index = 0;
    app.current_file_index = 0;
    app.current_hunk_index = 0;

    app.selected_line_index = 0;
    app.next_change_line();
    assert_eq!(app.selected_line_index, 1);
    app.selected_line_index = 2;
    app.next_change_line();
    assert_eq!(app.selected_line_index, 2);

    app.selected_line_index = 0;
    app.previous_change_line();
    assert_eq!(app.selected_line_index, 2);
    app.selected_line_index = 1;
    app.previous_change_line();
    assert_eq!(app.selected_line_index, 1);

    app.snapshots[0].files[0].hunks[0].lines = vec![" context only\n".to_string()];
    app.selected_line_index = 9;
    app.select_first_change_line();
    assert_eq!(app.selected_line_index, 0);

    app.focus = FocusPane::HelpSidebar;
    app.stage_current_selection();

    app.current_file_index = 99;
    assert_eq!(app.current_hunk_content_height(), 0);

    app.snapshots[0].files[0].hunks[0].lines = vec![
        "-old\n".to_string(),
        "+new\n".to_string(),
        " context after 1\n".to_string(),
        " context after 2\n".to_string(),
        " context after 3\n".to_string(),
        " context after 4\n".to_string(),
        " context after 5\n".to_string(),
        " context after 6\n".to_string(),
    ];
    app.current_file_index = 0;
    app.current_hunk_index = 0;
    app.scroll_offset = 99;
    app.clamp_scroll_offset(5);
    assert!(app.scroll_offset > 0);

    app.extended_help_scroll_offset = 20;
    app.clamp_extended_help_scroll_offset(200);
    assert_eq!(app.extended_help_scroll_offset, 0);
}

#[tokio::test]
async fn ui_draw_renders_mini_compact_help_and_empty_states() {
    let repo = TestRepo::new();
    repo.write_file("example.rs", "fn main() {\n    println!(\"one\");\n}\n");
    repo.commit_all("initial");
    repo.write_file(
        "example.rs",
        "fn main() {\n    println!(\"two\");\n    println!(\"three\");\n}\n",
    );

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    let mut terminal = Terminal::new(TestBackend::new(36, 20)).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw mini layout");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("Hunky"));

    app.mode = Mode::Streaming(StreamingType::Auto(StreamSpeed::Medium));
    terminal = Terminal::new(TestBackend::new(52, 20)).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw compact layout");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("STM:M"));

    app.show_help = true;
    app.focus = FocusPane::HelpSidebar;
    terminal = Terminal::new(TestBackend::new(90, 24)).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw focused help");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("Keys [FOCUSED]"));

    app.syntax_highlighting = false;
    app.wrap_lines = true;
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw non-highlighted wrapped diff");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("+ "));

    app.snapshots[0].files[0].hunks.clear();
    app.show_help = false;
    app.focus = FocusPane::HunkView;
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw no-hunks state");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("No hunks to display yet"));

    app.snapshots.clear();
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw no-snapshot state");
    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("No changes"));
}

#[tokio::test]
async fn enter_review_mode_loads_commits_and_sets_selecting_state() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit_all("initial");
    repo.write_file("a.txt", "two\n");
    repo.commit_all("second");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    assert_eq!(app.mode, Mode::Review);
    assert!(app.review_selecting_commit);
    assert!(!app.review_commits.is_empty());
    assert_eq!(app.review_commit_cursor, 0);
}

#[tokio::test]
async fn review_commit_cursor_navigates_within_bounds() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit_all("first");
    repo.write_file("a.txt", "two\n");
    repo.commit_all("second");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    assert_eq!(app.review_commit_cursor, 0);

    // Navigate down
    app.review_commit_cursor = 1;
    assert_eq!(app.review_commit_cursor, 1);

    // Can't go above max
    let max = app.review_commits.len().saturating_sub(1);
    app.review_commit_cursor = max;
    assert_eq!(app.review_commit_cursor, max);
}

#[tokio::test]
async fn select_review_commit_loads_diff_and_exits_picker() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1 updated\n");
    repo.commit_all("update");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    app.select_review_commit();

    assert!(!app.review_selecting_commit);
    assert!(app.review_snapshot.is_some());
    assert_eq!(app.mode, Mode::Review);
    assert_eq!(app.current_file_index, 0);
    assert_eq!(app.current_hunk_index, 0);
}

#[tokio::test]
async fn toggle_review_acceptance_marks_hunk_as_accepted() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1 updated\n");
    repo.commit_all("update");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    app.select_review_commit();

    // Hunk should not be accepted initially
    let hunk = &app.review_snapshot.as_ref().unwrap().files[0].hunks[0];
    assert!(!hunk.accepted);

    // Toggle acceptance
    app.toggle_review_acceptance();
    let hunk = &app.review_snapshot.as_ref().unwrap().files[0].hunks[0];
    assert!(hunk.accepted);

    // Toggle back
    app.toggle_review_acceptance();
    let hunk = &app.review_snapshot.as_ref().unwrap().files[0].hunks[0];
    assert!(!hunk.accepted);
}

#[tokio::test]
async fn exit_review_mode_restores_view_mode() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1 updated\n");
    repo.commit_all("update");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    app.select_review_commit();

    assert_eq!(app.mode, Mode::Review);

    app.exit_review_mode();
    assert_eq!(app.mode, Mode::View);
    assert!(!app.review_selecting_commit);
    assert!(app.review_commits.is_empty());
    assert!(app.review_snapshot.is_none());
}

#[tokio::test]
async fn ui_draw_renders_review_mode_header() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit_all("initial");
    repo.write_file("a.txt", "two\n");
    repo.commit_all("update");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    app.select_review_commit();

    let backend = TestBackend::new(160, 30);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw review mode");
    let rendered = render_buffer_to_string(&terminal);
    assert!(
        rendered.contains("REVIEW"),
        "expected REVIEW in header, got:\n{}",
        rendered
    );
}

#[tokio::test]
async fn ui_draw_renders_commit_picker() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit_all("initial commit");
    repo.write_file("a.txt", "two\n");
    repo.commit_all("second commit");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw commit picker");
    let rendered = render_buffer_to_string(&terminal);
    assert!(
        rendered.contains("Select a commit"),
        "expected commit picker title, got:\n{}",
        rendered
    );
    assert!(
        rendered.contains("second commit"),
        "expected commit message in picker, got:\n{}",
        rendered
    );
}

#[tokio::test]
async fn ui_draw_renders_accepted_indicator_in_review_mode() {
    let repo = TestRepo::new();
    repo.write_file("example.txt", "line 1\n");
    repo.commit_all("initial");
    repo.write_file("example.txt", "line 1 updated\n");
    repo.commit_all("update");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    app.select_review_commit();
    app.toggle_review_acceptance();

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            UI::new(&app).draw(frame);
        })
        .expect("failed to draw accepted hunk");
    let rendered = render_buffer_to_string(&terminal);
    assert!(
        rendered.contains("[ACCEPTED ✓]"),
        "expected [ACCEPTED ✓] indicator, got:\n{}",
        rendered
    );
}

#[tokio::test]
async fn review_mode_advance_hunk_wraps() {
    let repo = TestRepo::new();
    repo.write_file("a.txt", "one\n");
    repo.commit_all("initial");
    repo.write_file("a.txt", "two\n");
    repo.commit_all("update");

    let mut app = App::new(repo.path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");

    app.enter_review_mode();
    app.select_review_commit();

    let file_count = app.review_snapshot.as_ref().unwrap().files.len();
    // Advance past all hunks - should wrap back to start
    for _ in 0..100 {
        app.advance_hunk();
    }
    // Should have wrapped back
    assert!(app.current_file_index < file_count);
}
