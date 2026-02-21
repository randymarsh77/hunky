use super::*;
use ratatui::{backend::TestBackend, Terminal};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

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

fn init_temp_repo() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("failed to get system time")
        .as_nanos();
    let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "hunky-ui-tests-{}-{}-{}",
        std::process::id(),
        unique,
        counter
    ));

    fs::create_dir_all(&path).expect("failed to create temp directory");
    let output = Command::new("git")
        .arg("init")
        .current_dir(&path)
        .output()
        .expect("failed to initialize git repo");
    assert!(output.status.success(), "git init failed");

    path
}

#[tokio::test]
async fn draw_renders_header_and_empty_state() {
    let repo_path = init_temp_repo();
    let app = App::new(repo_path.to_str().expect("path should be utf-8"))
        .await
        .expect("failed to create app");
    let ui = UI::new(&app);

    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");
    terminal
        .draw(|frame| {
            ui.draw(frame);
        })
        .expect("failed to draw ui");

    let rendered = render_buffer_to_string(&terminal);
    assert!(rendered.contains("Hunky"));
    assert!(rendered.contains("Files"));

    fs::remove_dir_all(repo_path).expect("failed to remove temp repo");
}

#[test]
fn fade_color_dims_rgb_values() {
    assert_eq!(fade_color(Color::Rgb(200, 100, 50)), Color::Rgb(80, 40, 20));
    assert_eq!(fade_color(Color::Blue), Color::DarkGray);
}
