//! Startup splash: the title and a little diff hunk rain down onto the screen,
//! a diagonal shimmer sweeps across them, and then everything falls away.
//!
//! The animator only depends on ratatui, so the native terminal loop and the
//! browser build drive the same code: native calls [`SplashAnimator::tick`] at
//! roughly 30 fps, while the browser advances by elapsed host time.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    Frame,
};

/// Duration of one animation frame.
pub const FRAME_MS: u32 = 33;

const HINT: &str = "Press any key to skip";

/// Big block letter "HUNKY".
const TITLE_ART: &[&str] = &[
    "██   ██ ██   ██ ███   ██ ██   ██ ██    ██",
    "██   ██ ██   ██ ████  ██ ██  ██   ██  ██ ",
    "███████ ██   ██ ██ ██ ██ █████     ████  ",
    "██   ██ ██   ██ ██  ████ ██  ██     ██   ",
    "██   ██  █████  ██   ███ ██   ██    ██   ",
];

/// A small diff hunk. Letters are colour codes (see [`hunk_style`]); everything
/// else is drawn literally in the header colour.
const HUNK_ART: &[&str] = &[
    "@@ -12,6 +12,9 @@",
    "",
    "  ccccccc cccccc",
    "    cccc ccccccccc",
    "M   rrrrrr rrrrrrr",
    "M   rrrr rrrrr",
    "P   aaaaaa aaaaaaaaa",
    "P   aaaa aaaaaaa aaa",
    "P   aaaaaaaa aaaa",
    "    cccccc",
    "  ccc",
];

const TITLE_COLOR: Color = Color::Cyan;
const SHIMMER_COLOR: Color = Color::White;

fn hunk_style(ch: char) -> (char, Style) {
    match ch {
        'c' => ('▀', Style::default().fg(Color::DarkGray)),
        'r' => ('▀', Style::default().fg(Color::Red)),
        'a' => ('▀', Style::default().fg(Color::Green)),
        'M' => ('-', Style::default().fg(Color::Red)),
        'P' => ('+', Style::default().fg(Color::Green)),
        other => (other, Style::default().fg(Color::Cyan)),
    }
}

/// A styled glyph positioned relative to the top-left of the content block.
struct Glyph {
    ch: char,
    style: Style,
    col: u16,
    row: u16,
}

fn art_width(lines: &[&str]) -> usize {
    lines.iter().map(|l| l.chars().count()).max().unwrap_or(0)
}

/// Lay out the title beside the hunk, or the title alone when the terminal is
/// too narrow for both.
fn build_content(width: u16) -> (Vec<Glyph>, u16, u16) {
    const GAP: usize = 6;
    let title_w = art_width(TITLE_ART);
    let hunk_w = art_width(HUNK_ART);
    let with_hunk = title_w + GAP + hunk_w <= width as usize;

    let height = if with_hunk {
        TITLE_ART.len().max(HUNK_ART.len())
    } else {
        TITLE_ART.len()
    };
    let title_top = (height - TITLE_ART.len()) / 2;

    let mut glyphs = Vec::new();
    for (row, line) in TITLE_ART.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch != ' ' {
                glyphs.push(Glyph {
                    ch,
                    style: Style::default().fg(TITLE_COLOR),
                    col: col as u16,
                    row: (title_top + row) as u16,
                });
            }
        }
    }
    let mut total_w = title_w;
    if with_hunk {
        let left = title_w + GAP;
        let hunk_top = (height - HUNK_ART.len()) / 2;
        for (row, line) in HUNK_ART.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if ch != ' ' {
                    let (ch, style) = hunk_style(ch);
                    glyphs.push(Glyph {
                        ch,
                        style,
                        col: (left + col) as u16,
                        row: (hunk_top + row) as u16,
                    });
                }
            }
        }
        total_w = left + hunk_w;
    }
    (glyphs, total_w as u16, height as u16)
}

/// Small xorshift generator so the animation needs no extra dependencies and is
/// reproducible from a seed (the browser host supplies one).
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (self.next() as f32 / u32::MAX as f32) * (hi - lo)
    }

    fn below(&mut self, n: u32) -> u16 {
        (self.next() % n) as u16
    }
}

struct Cell {
    glyph: Glyph,
    /// Vertical position relative to the content's resting top row.
    y: f32,
    speed_in: f32,
    delay_in: u16,
    speed_out: f32,
    delay_out: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    FallingIn,
    HoldPreShimmer,
    Shimmer,
    HoldPostShimmer,
    FallingOut,
    Done,
}

const HOLD_TICKS: u16 = 10;
const SHIMMER_WIDTH: f32 = 6.0;
const SHIMMER_SPEED: f32 = 4.0;

/// Falling-glyph startup animation with a diagonal shimmer.
///
///   1. **Fall in**: glyphs drop from above the screen at random speeds.
///   2. **Hold**: brief pause.
///   3. **Shimmer**: a white diagonal band sweeps left to right.
///   4. **Hold**: brief pause.
///   5. **Fall out**: glyphs drop off the bottom in a random order.
pub struct SplashAnimator {
    cells: Vec<Cell>,
    content_w: u16,
    content_h: u16,
    area: Rect,
    phase: Phase,
    tick: u64,
    out_tick: u64,
    hold: u16,
    shimmer_x: f32,
    shimmer_end: f32,
    /// Leftover host time not yet turned into whole frames.
    pending_ms: f64,
}

impl SplashAnimator {
    pub fn new(area: Rect, seed: u32) -> Self {
        let mut rng = Rng(seed.max(1));
        let (glyphs, content_w, content_h) = build_content(area.width);
        let top = area.height.saturating_sub(content_h) / 2;
        let cells = glyphs
            .into_iter()
            .map(|glyph| Cell {
                glyph,
                // Start somewhere above the visible screen.
                y: rng.range(-(area.height as f32), 0.0) - top as f32 - 1.0,
                speed_in: rng.range(1.6, 5.0),
                delay_in: rng.below(4),
                speed_out: rng.range(1.6, 6.0),
                delay_out: rng.below(10),
            })
            .collect();
        let shimmer_end = content_w as f32 + content_h as f32 + SHIMMER_WIDTH;
        Self {
            cells,
            content_w,
            content_h,
            area,
            phase: Phase::FallingIn,
            tick: 0,
            out_tick: 0,
            hold: HOLD_TICKS,
            shimmer_x: -SHIMMER_WIDTH,
            shimmer_end,
            pending_ms: 0.0,
        }
    }

    /// Seed from the clock, for the native binary.
    #[cfg(not(feature = "browser"))]
    pub fn from_clock(area: Rect) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(1);
        Self::new(area, nanos)
    }

    /// Track the latest terminal size so the content stays centred.
    pub fn set_area(&mut self, area: Rect) {
        self.area = area;
    }

    fn origin(&self) -> (u16, u16) {
        (
            self.area.width.saturating_sub(self.content_w) / 2,
            self.area.height.saturating_sub(self.content_h) / 2,
        )
    }

    /// Advance by host time, stepping whole frames and carrying the remainder.
    pub fn advance(&mut self, elapsed_ms: f64) {
        self.pending_ms += elapsed_ms.max(0.0);
        while self.pending_ms >= FRAME_MS as f64 && !self.is_done() {
            self.pending_ms -= FRAME_MS as f64;
            self.tick();
        }
    }

    /// Advance the animation by one frame.
    pub fn tick(&mut self) {
        self.tick += 1;
        match self.phase {
            Phase::FallingIn => {
                let mut landed = true;
                for cell in &mut self.cells {
                    let rest = cell.glyph.row as f32;
                    if self.tick <= cell.delay_in as u64 {
                        landed = false;
                    } else if cell.y < rest {
                        cell.y = (cell.y + cell.speed_in).min(rest);
                        landed &= cell.y >= rest;
                    }
                }
                if landed {
                    self.phase = Phase::HoldPreShimmer;
                }
            }
            Phase::HoldPreShimmer | Phase::HoldPostShimmer => {
                self.hold = self.hold.saturating_sub(1);
                if self.hold == 0 {
                    self.phase = if self.phase == Phase::HoldPreShimmer {
                        Phase::Shimmer
                    } else {
                        Phase::FallingOut
                    };
                }
            }
            Phase::Shimmer => {
                self.shimmer_x += SHIMMER_SPEED;
                if self.shimmer_x > self.shimmer_end {
                    self.hold = HOLD_TICKS;
                    self.phase = Phase::HoldPostShimmer;
                }
            }
            Phase::FallingOut => {
                self.out_tick += 1;
                let (_, top) = self.origin();
                let bottom = self.area.height as f32 - top as f32;
                let mut gone = true;
                for cell in &mut self.cells {
                    if self.out_tick <= cell.delay_out as u64 {
                        gone = false;
                    } else if cell.y <= bottom {
                        cell.y += cell.speed_out;
                        gone &= cell.y > bottom;
                    }
                }
                if gone {
                    self.phase = Phase::Done;
                }
            }
            Phase::Done => {}
        }
    }

    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Draw the current frame over the whole of `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        self.render_to_buffer(frame.buffer_mut(), area);
    }

    pub fn render_to_buffer(&self, buf: &mut Buffer, area: Rect) {
        buf.set_style(area, Style::reset());
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_symbol(" ");
            }
        }

        let left = area.width.saturating_sub(self.content_w) / 2;
        let top = area.height.saturating_sub(self.content_h) / 2;
        let shimmering = self.phase == Phase::Shimmer;
        let mut symbol = [0u8; 4];
        for cell in &self.cells {
            let y = top as i32 + cell.y.round() as i32;
            let x = left as u32 + cell.glyph.col as u32;
            if y < 0 || y >= area.height as i32 || x >= area.width as u32 {
                continue;
            }
            // The band leans left as it goes down, like light across glass.
            let style = if shimmering {
                let band = self.shimmer_x - cell.glyph.row as f32;
                let col = cell.glyph.col as f32;
                if col >= band && col < band + SHIMMER_WIDTH {
                    Style::default().fg(SHIMMER_COLOR)
                } else {
                    cell.glyph.style
                }
            } else {
                cell.glyph.style
            };
            buf[(area.x + x as u16, area.y + y as u16)]
                .set_symbol(cell.glyph.ch.encode_utf8(&mut symbol))
                .set_style(style);
        }

        if area.height >= 2 && area.width as usize >= HINT.len() {
            let x = area.x + (area.width - HINT.len() as u16) / 2;
            let y = area.bottom() - 2;
            buf.set_string(x, y, HINT, Style::default().fg(Color::DarkGray));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buf: &Buffer) -> String {
        buf.content().iter().map(|c| c.symbol()).collect()
    }

    fn run(anim: &mut SplashAnimator) -> Vec<Phase> {
        let mut phases = vec![anim.phase];
        for _ in 0..1000 {
            if anim.is_done() {
                break;
            }
            anim.tick();
            if phases.last() != Some(&anim.phase) {
                phases.push(anim.phase);
            }
        }
        phases
    }

    #[test]
    fn plays_every_phase_in_order_and_finishes_quickly() {
        let area = Rect::new(0, 0, 100, 30);
        let mut anim = SplashAnimator::new(area, 7);
        assert_eq!(
            run(&mut anim),
            vec![
                Phase::FallingIn,
                Phase::HoldPreShimmer,
                Phase::Shimmer,
                Phase::HoldPostShimmer,
                Phase::FallingOut,
                Phase::Done
            ]
        );
        // Keep the whole thing brief: well under four seconds at 30 fps.
        assert!(anim.tick < 110, "took {} ticks", anim.tick);
    }

    #[test]
    fn settles_title_and_hunk_centred_then_shimmers_white() {
        let area = Rect::new(0, 0, 100, 30);
        let mut anim = SplashAnimator::new(area, 3);
        while anim.phase == Phase::FallingIn {
            anim.tick();
        }
        let mut buf = Buffer::empty(area);
        anim.render_to_buffer(&mut buf, area);
        let rendered = text(&buf);
        assert!(rendered.contains("@@ -12,6 +12,9 @@"));
        assert!(rendered.contains(HINT));
        assert!(buf
            .content()
            .iter()
            .any(|c| c.symbol() == "█" && c.fg == TITLE_COLOR));
        assert!(!buf.content().iter().any(|c| c.fg == SHIMMER_COLOR));

        while anim.phase != Phase::Shimmer {
            anim.tick();
        }
        for _ in 0..4 {
            anim.tick();
        }
        anim.render_to_buffer(&mut buf, area);
        assert!(buf
            .content()
            .iter()
            .any(|c| c.symbol() == "█" && c.fg == SHIMMER_COLOR));
    }

    #[test]
    fn narrow_terminals_show_only_the_title() {
        let area = Rect::new(0, 0, 50, 12);
        let mut anim = SplashAnimator::new(area, 9);
        while anim.phase == Phase::FallingIn {
            anim.tick();
        }
        let mut buf = Buffer::empty(area);
        anim.render_to_buffer(&mut buf, area);
        assert!(!text(&buf).contains("@@"));
        assert!(text(&buf).contains('█'));
    }

    #[test]
    fn advance_steps_whole_frames_and_carries_the_remainder() {
        let area = Rect::new(0, 0, 100, 30);
        let mut anim = SplashAnimator::new(area, 5);
        anim.advance(FRAME_MS as f64 * 2.5);
        assert_eq!(anim.tick, 2);
        anim.advance(FRAME_MS as f64 * 0.5);
        assert_eq!(anim.tick, 3);
        anim.advance(1_000_000.0);
        assert!(anim.is_done());
    }

    #[test]
    fn tiny_or_resized_areas_do_not_panic() {
        let mut anim = SplashAnimator::new(Rect::new(0, 0, 1, 1), 1);
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        for _ in 0..200 {
            let area = buf.area;
            anim.render_to_buffer(&mut buf, area);
            anim.tick();
        }
        let mut anim = SplashAnimator::new(Rect::new(0, 0, 120, 40), 1);
        anim.set_area(Rect::new(0, 0, 20, 5));
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        for _ in 0..200 {
            let area = buf.area;
            anim.render_to_buffer(&mut buf, area);
            anim.tick();
        }
        assert!(anim.is_done());
    }
}
