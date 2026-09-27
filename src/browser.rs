use std::{cell::RefCell, time::Duration};

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::Paragraph,
    Frame,
};
use serde::Deserialize;
use tui2web::{
    app::{AppResult, Application, Context, Input, Update},
    fs::Filesystem,
    git::GitRepository,
};

use crate::{
    app::{App as HunkyApp, Mode, StreamingType},
    git::GitRepo,
    input::{KeyCode, KeyEvent, KeyModifiers},
    splash::{SplashAnimator, FRAME_MS},
    ui::UI,
};

pub struct BrowserApp {
    app: RefCell<HunkyApp>,
    git: GitRepo,
    message: String,
    last_advance: f64,
    /// The startup animation, until it finishes or a key skips it.
    splash: Option<SplashAnimator>,
    last_splash_tick: f64,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
enum DemoCommand {
    Edit { path: String, content: String },
    Delete { path: String },
    Commit { message: String },
}

impl BrowserApp {
    fn command(&mut self, text: &str) -> anyhow::Result<()> {
        let command: DemoCommand = serde_json::from_str(text)?;
        let mut repository = self.git.repository.borrow_mut();
        match command {
            DemoCommand::Edit { path, content } => {
                repository
                    .filesystem_mut()
                    .write_file(&path, content.as_bytes())?;
                self.message = format!("Edited {path}; the index is unchanged.");
            }
            DemoCommand::Delete { path } => {
                repository.filesystem_mut().remove_file(&path)?;
                self.message = format!("Deleted {path} from the working tree only.");
            }
            DemoCommand::Commit { message } => {
                if message.trim().is_empty() {
                    anyhow::bail!("Enter a commit message");
                }
                let id = repository.commit(message.trim(), "Browser demo")?;
                self.message = format!("Created simulated commit {id}; no remote was contacted.");
            }
        }
        drop(repository);
        let mut app = self.app.borrow_mut();
        if matches!(app.mode(), Mode::Streaming(_)) {
            app.ingest_snapshot(self.git.get_diff_snapshot()?);
        } else {
            app.refresh_current_snapshot_from_git();
        }
        Ok(())
    }

    fn publish_state(&self, context: &mut Context) -> AppResult<()> {
        let repository = self.git.repository.borrow();
        let app = self.app.borrow();
        let files = repository
            .filesystem()
            .list_files()
            .into_iter()
            .map(|path| {
                repository.filesystem().read_file(&path).map(|bytes| {
                serde_json::json!({"path": path, "content": String::from_utf8_lossy(&bytes)})
            })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let diff = |files: Vec<tui2web::git::FileDiff>| {
            files
                .into_iter()
                .map(|file| {
                    format!(
                        "--- {}\n{}",
                        file.path,
                        file.hunks
                            .into_iter()
                            .map(|hunk| hunk.lines.join(""))
                            .collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let current = app.current_file().map(|file| serde_json::json!({
            "path": file.path,
            "hunk": app.current_hunk_index(),
            "hunks": file.hunks.len(),
            "lineMode": app.line_selection_mode(),
            "selectedLine": app.selected_line_index(),
            "stagedLines": file.hunks.iter().map(|hunk| hunk.staged_line_indices.len()).collect::<Vec<_>>(),
            "indexOnly": file.hunks.iter().map(|hunk| hunk.id.index_only).collect::<Vec<_>>(),
        }));
        // The read-only inspector includes the complete repository, never just the worktree.
        let state = serde_json::json!({
            "repository": repository.snapshot(),
            "files": files,
            "stagedDiff": diff(repository.diff_staged().map_err(|error| error.to_string())?),
            "unstagedDiff": diff(repository.diff_unstaged().map_err(|error| error.to_string())?),
            "current": current,
            "message": self.message,
        });
        context
            .fs
            .write_file(
                "/demo.json",
                &serde_json::to_vec(&state).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
    }
}

impl Application for BrowserApp {
    fn init(context: &mut Context) -> AppResult<Self> {
        let git = GitRepo::fixture().map_err(|error| error.to_string())?;
        let mut app = HunkyApp::with_repository(git.clone()).map_err(|error| error.to_string())?;
        let file_count = app
            .current_snapshot()
            .map_or(0, |snapshot| snapshot.files.len());
        for _ in 0..file_count {
            if app
                .current_file()
                .is_some_and(|file| file.path.to_str() == Some("src/shop.rs"))
            {
                break;
            }
            app.handle_key(KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::default(),
            });
        }
        let demo = Self {
            app: RefCell::new(app),
            git,
            message: "Browser-local simulated Git. S stages a hunk; L selects individual lines."
                .into(),
            last_advance: context.now_ms,
            // Hosts can pass `splash: "off"` in the init config to start on the diff view.
            splash: (context.config.get("splash").map(String::as_str) != Some("off")).then(|| {
                let area = Rect::new(0, 0, context.columns, context.rows);
                SplashAnimator::new(area, context.random_u32())
            }),
            last_splash_tick: context.now_ms,
        };
        demo.publish_state(context)?;
        Ok(demo)
    }

    fn initial_wake_after_ms(&self) -> Option<u32> {
        self.splash.as_ref().map(|_| FRAME_MS)
    }

    fn update(&mut self, input: Input, context: &mut Context) -> AppResult<Update> {
        let mut exit = false;
        let mut dirty = true;
        let mut redraw = false;
        // A key skips the splash and still reaches Hunky, so typing never feels ignored.
        if matches!(input, Input::Key { .. } | Input::Text { .. }) && self.splash.take().is_some() {
            redraw = true;
        }
        match input {
            Input::Key { key, modifiers, .. } => {
                let code = match key.as_str() {
                    "ArrowUp" => Some(KeyCode::Up),
                    "ArrowDown" => Some(KeyCode::Down),
                    "Enter" => Some(KeyCode::Enter),
                    "Escape" => Some(KeyCode::Esc),
                    "Tab" if modifiers.shift => Some(KeyCode::BackTab),
                    "Tab" => Some(KeyCode::Tab),
                    _ if key.chars().count() == 1 => key.chars().next().map(KeyCode::Char),
                    _ => None,
                };
                if let Some(code) = code {
                    let mut app = self.app.borrow_mut();
                    let previous_mode = app.mode();
                    exit = app.handle_key(KeyEvent {
                        code,
                        modifiers: KeyModifiers(
                            u8::from(modifiers.shift) | (u8::from(modifiers.ctrl) << 1),
                        ),
                    });
                    if app.mode() != previous_mode {
                        self.last_advance = context.now_ms;
                    }
                } else {
                    dirty = false;
                }
                if let Some(error) = crate::logger::take_error() {
                    self.message = error;
                }
            }
            Input::Paste { text } => {
                if let Err(error) = self.command(&text) {
                    self.message = format!("Error: {error}");
                }
            }
            Input::Text { text } => {
                let mut app = self.app.borrow_mut();
                let previous_mode = app.mode();
                for character in text.chars() {
                    exit = app.handle_key(KeyEvent {
                        code: KeyCode::Char(character),
                        modifiers: KeyModifiers::default(),
                    });
                    if exit {
                        break;
                    }
                }
                if app.mode() != previous_mode {
                    self.last_advance = context.now_ms;
                }
                if let Some(error) = crate::logger::take_error() {
                    self.message = error;
                }
            }
            Input::Tick if self.splash.is_some() => {
                let splash = self.splash.as_mut().expect("splash is active");
                splash.set_area(Rect::new(0, 0, context.columns, context.rows));
                splash.advance(context.now_ms - self.last_splash_tick);
                self.last_splash_tick = context.now_ms;
                if splash.is_done() {
                    self.splash = None;
                    self.last_advance = context.now_ms;
                }
                dirty = false;
                redraw = true;
            }
            Input::Tick => {
                let elapsed =
                    Duration::from_millis((context.now_ms - self.last_advance).max(0.0) as u64);
                dirty = self.app.borrow_mut().tick(elapsed);
                if dirty {
                    self.last_advance = context.now_ms;
                }
            }
            _ => dirty = false,
        }
        if dirty {
            self.publish_state(context)?;
        }
        let auto = matches!(
            self.app.borrow().mode(),
            Mode::Streaming(StreamingType::Auto(_))
        );
        let wake_after_ms = if self.splash.is_some() {
            Some(FRAME_MS)
        } else {
            auto.then_some(100)
        };
        Ok(Update {
            dirty: dirty || redraw,
            files_changed: dirty,
            exit,
            wake_after_ms,
        })
    }

    fn render(&self, frame: &mut Frame, _context: &Context) {
        if let Some(splash) = &self.splash {
            splash.render(frame, frame.area());
            return;
        }
        let mut app = self.app.borrow_mut();
        let (diff, help, _) = UI::new(&app).draw(frame);
        app.update_viewports(diff, help);
        let area = frame.area();
        if area.height > 0 {
            frame.render_widget(
                Paragraph::new(self.message.as_str())
                    .style(Style::default().fg(Color::Yellow).bg(Color::Black)),
                Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
        }
    }
}

tui2web::export_app!(BrowserApp);

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use tui2web::app::{Command, Runner};

    fn runner() -> Runner<BrowserApp> {
        Runner::from_json(
            &json!({
                "version": 1, "columns": 100, "rows": 30, "nowMs": 0,
                "randomSeed": 1, "snapshot": null
            })
            .to_string(),
        )
        .unwrap()
    }

    fn splash_runner(config: Value) -> Runner<BrowserApp> {
        Runner::from_json(
            &json!({
                "version": 1, "columns": 100, "rows": 30, "nowMs": 0,
                "randomSeed": 1, "snapshot": null, "config": config
            })
            .to_string(),
        )
        .unwrap()
    }

    fn state(runner: &mut Runner<BrowserApp>) -> Value {
        let snapshot = runner
            .dispatch(Command::Snapshot)
            .unwrap()
            .snapshot
            .unwrap();
        serde_json::from_slice(
            &snapshot
                .files
                .iter()
                .find(|(path, _)| path == "demo.json")
                .unwrap()
                .1,
        )
        .unwrap()
    }

    fn key(runner: &mut Runner<BrowserApp>, key: &str) {
        runner
            .dispatch(Command::Event {
                input: Input::Key {
                    key: key.into(),
                    code: String::new(),
                    repeat: false,
                    modifiers: Default::default(),
                },
                now_ms: 0.0,
            })
            .unwrap();
    }

    fn command(runner: &mut Runner<BrowserApp>, command: Value) {
        runner
            .dispatch(Command::Event {
                input: Input::Paste {
                    text: command.to_string(),
                },
                now_ms: 0.0,
            })
            .unwrap();
    }

    #[test]
    fn splash_plays_on_ticks_then_shows_hunky_and_stops_waking() {
        let mut app = splash_runner(json!({}));
        let initial = app.initial_output().unwrap();
        assert_eq!(initial.wake_after_ms, Some(FRAME_MS));
        assert!(initial.frame.unwrap().contains("any key to skip"));
        let mut now = 0.0;
        let mut frames = 0;
        let last = loop {
            now += FRAME_MS as f64;
            let output = app
                .dispatch(Command::Event {
                    input: Input::Tick,
                    now_ms: now,
                })
                .unwrap();
            frames += 1;
            // Splash frames repaint without republishing the demo state.
            assert!(output.snapshot.is_none());
            if output.wake_after_ms.is_none() || frames > 500 {
                break output;
            }
        };
        assert!(frames < 150, "splash took {frames} frames");
        assert!(last.frame.unwrap().contains("Hunky"));
    }

    #[test]
    fn key_skips_splash_and_still_reaches_hunky() {
        let mut app = splash_runner(json!({}));
        app.initial_output().unwrap();
        let output = app
            .dispatch(Command::Event {
                input: Input::Text { text: "l".into() },
                now_ms: 0.0,
            })
            .unwrap();
        assert!(output.frame.unwrap().contains("Hunky"));
        assert_eq!(output.wake_after_ms, None);
        assert_eq!(state(&mut app)["current"]["lineMode"], true);
    }

    #[test]
    fn splash_can_be_disabled_by_host_config() {
        let mut app = splash_runner(json!({"splash": "off"}));
        let initial = app.initial_output().unwrap();
        assert_eq!(initial.wake_after_ms, None);
        assert!(initial.frame.unwrap().contains("Hunky"));
    }

    #[test]
    fn real_app_stages_lines_hunks_and_keeps_instances_isolated() {
        let mut first = runner();
        let mut second = runner();
        assert!(!first
            .initial_output()
            .unwrap()
            .frame
            .unwrap()
            .contains("Hunky"));
        let initial = state(&mut first);
        assert_eq!(initial["current"]["path"], "src/shop.rs");
        assert_eq!(initial["current"]["hunks"], 2);
        assert!(!initial["stagedDiff"]
            .as_str()
            .unwrap()
            .contains("Browser edits stay local."));
        assert!(initial["unstagedDiff"]
            .as_str()
            .unwrap()
            .contains("Browser edits stay local."));
        key(&mut first, "l");
        key(&mut first, "s");
        let partial = state(&mut first);
        assert_eq!(partial["current"]["stagedLines"][0], 1);
        assert_eq!(state(&mut second)["repository"], initial["repository"]);
        key(&mut first, "s");
        assert_eq!(
            state(&mut first)["repository"]["index"],
            initial["repository"]["index"]
        );
        key(&mut first, "l");
        key(&mut first, "s");
        let staged = state(&mut first);
        assert_eq!(staged["current"]["stagedLines"][0], 2);
        assert_eq!(staged["current"]["stagedLines"][1], 0);
        assert!(staged["stagedDiff"]
            .as_str()
            .unwrap()
            .contains("Hello from Orchard!"));
        key(&mut first, "s");
        assert_eq!(
            state(&mut first)["repository"]["index"],
            initial["repository"]["index"]
        );
    }

    #[test]
    fn staged_edit_survives_worktree_revert_and_can_be_unstaged() {
        let mut app = runner();
        let before = state(&mut app);
        key(&mut app, "s");
        let staged_index = state(&mut app)["repository"]["index"].clone();
        command(
            &mut app,
            json!({"action":"edit", "path":"src/shop.rs", "content":include_str!("../browser-fixture/shop.head.rs")}),
        );
        let edited = state(&mut app);
        assert_eq!(edited["repository"]["index"], staged_index);
        assert!(edited["current"]["indexOnly"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == true));
        key(&mut app, "s");
        assert_eq!(
            state(&mut app)["repository"]["index"],
            before["repository"]["index"]
        );
    }

    #[test]
    fn edit_commit_delete_and_invalid_commands_are_observable() {
        let mut app = runner();
        let before = state(&mut app);
        command(
            &mut app,
            json!({"action":"edit","path":"notes.txt","content":"Changed by visitor\n"}),
        );
        assert_eq!(
            state(&mut app)["repository"]["index"],
            before["repository"]["index"]
        );
        command(
            &mut app,
            json!({"action":"commit","message":"Visitor commit"}),
        );
        assert_eq!(
            state(&mut app)["repository"]["commits"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        command(&mut app, json!({"action":"delete","path":"notes.txt"}));
        assert!(!state(&mut app)["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"] == "notes.txt"));
        command(
            &mut app,
            json!({"action":"edit","path":"../../escape","content":"bad"}),
        );
        assert!(state(&mut app)["message"]
            .as_str()
            .unwrap()
            .starts_with("Error:"));
        assert_eq!(state(&mut runner())["repository"], before["repository"]);
    }

    #[test]
    fn working_tree_edits_feed_streaming_and_idle_events_do_not_redraw() {
        let mut app = runner();
        let idle = app
            .dispatch(Command::Event {
                input: Input::Focus { focused: true },
                now_ms: 0.0,
            })
            .unwrap();
        assert!(idle.frame.is_none());
        assert!(idle.snapshot.is_none());
        app.dispatch(Command::Event {
            input: Input::Text { text: "m".into() },
            now_ms: 0.0,
        })
        .unwrap();
        command(
            &mut app,
            json!({"action":"edit","path":"notes.txt","content":"A new streamed edit\n"}),
        );
        assert_eq!(state(&mut app)["current"]["path"], "README.md");
        assert!(state(&mut app)["unstagedDiff"]
            .as_str()
            .unwrap()
            .contains("A new streamed edit"));
    }
}
