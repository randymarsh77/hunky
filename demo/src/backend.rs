//! A ratatui 0.29-compatible [`Backend`] that renders frames as ANSI strings.
//!
//! This is a thin adaptation of the backend from the `tui2web` crate, updated
//! to implement the ratatui 0.29 [`Backend`] trait so the real hunky UI code
//! can be used directly.

use ratatui::{
    backend::{Backend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
    style::{Color, Modifier},
};
use std::io;

/// A ratatui [`Backend`] that renders terminal frames as ANSI escape-code
/// strings suitable for display in a web-based terminal emulator such as
/// xterm.js.
pub struct WebBackend {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
    cursor_x: u16,
    cursor_y: u16,
    cursor_visible: bool,
    ansi_output: String,
}

impl WebBackend {
    pub fn new(width: u16, height: u16) -> Self {
        WebBackend {
            width,
            height,
            cells: vec![Cell::default(); usize::from(width) * usize::from(height)],
            cursor_x: 0,
            cursor_y: 0,
            cursor_visible: true,
            ansi_output: String::new(),
        }
    }

    pub fn get_ansi_output(&self) -> &str {
        &self.ansi_output
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.cells = vec![Cell::default(); usize::from(width) * usize::from(height)];
    }

    fn render_to_ansi(&self) -> String {
        let capacity = usize::from(self.width) * usize::from(self.height) * 4;
        let mut out = String::with_capacity(capacity);

        out.push_str("\x1b[?25l");

        let mut prev_fg = Color::Reset;
        let mut prev_bg = Color::Reset;
        let mut prev_modifier = Modifier::empty();

        for y in 0..self.height {
            out.push_str("\x1b[");
            push_u16(&mut out, y + 1);
            out.push_str(";1H");

            for x in 0..self.width {
                let cell = &self.cells[usize::from(y) * usize::from(self.width) + usize::from(x)];
                let fg = cell.fg;
                let bg = cell.bg;
                let modifier = cell.modifier;

                if fg != prev_fg || bg != prev_bg || modifier != prev_modifier {
                    out.push_str("\x1b[0m");

                    if modifier.contains(Modifier::BOLD) {
                        out.push_str("\x1b[1m");
                    }
                    if modifier.contains(Modifier::DIM) {
                        out.push_str("\x1b[2m");
                    }
                    if modifier.contains(Modifier::ITALIC) {
                        out.push_str("\x1b[3m");
                    }
                    if modifier.contains(Modifier::UNDERLINED) {
                        out.push_str("\x1b[4m");
                    }
                    if modifier.contains(Modifier::SLOW_BLINK)
                        || modifier.contains(Modifier::RAPID_BLINK)
                    {
                        out.push_str("\x1b[5m");
                    }
                    if modifier.contains(Modifier::REVERSED) {
                        out.push_str("\x1b[7m");
                    }
                    if modifier.contains(Modifier::CROSSED_OUT) {
                        out.push_str("\x1b[9m");
                    }

                    if fg != Color::Reset {
                        push_fg_color(&mut out, fg);
                    }
                    if bg != Color::Reset {
                        push_bg_color(&mut out, bg);
                    }

                    prev_fg = fg;
                    prev_bg = bg;
                    prev_modifier = modifier;
                }

                out.push_str(cell.symbol());
            }
        }

        out.push_str("\x1b[0m");

        out.push_str("\x1b[");
        push_u16(&mut out, self.cursor_y + 1);
        out.push(';');
        push_u16(&mut out, self.cursor_x + 1);
        out.push('H');

        if self.cursor_visible {
            out.push_str("\x1b[?25h");
        }

        out
    }
}

fn push_u16(s: &mut String, n: u16) {
    if n >= 10000 {
        s.push((b'0' + (n / 10000) as u8) as char);
    }
    if n >= 1000 {
        s.push((b'0' + (n / 1000 % 10) as u8) as char);
    }
    if n >= 100 {
        s.push((b'0' + (n / 100 % 10) as u8) as char);
    }
    if n >= 10 {
        s.push((b'0' + (n / 10 % 10) as u8) as char);
    }
    s.push((b'0' + (n % 10) as u8) as char);
}

fn push_fg_color(out: &mut String, color: Color) {
    match color {
        Color::Reset => out.push_str("\x1b[39m"),
        Color::Black => out.push_str("\x1b[30m"),
        Color::Red => out.push_str("\x1b[31m"),
        Color::Green => out.push_str("\x1b[32m"),
        Color::Yellow => out.push_str("\x1b[33m"),
        Color::Blue => out.push_str("\x1b[34m"),
        Color::Magenta => out.push_str("\x1b[35m"),
        Color::Cyan => out.push_str("\x1b[36m"),
        Color::Gray => out.push_str("\x1b[37m"),
        Color::DarkGray => out.push_str("\x1b[90m"),
        Color::LightRed => out.push_str("\x1b[91m"),
        Color::LightGreen => out.push_str("\x1b[92m"),
        Color::LightYellow => out.push_str("\x1b[93m"),
        Color::LightBlue => out.push_str("\x1b[94m"),
        Color::LightMagenta => out.push_str("\x1b[95m"),
        Color::LightCyan => out.push_str("\x1b[96m"),
        Color::White => out.push_str("\x1b[97m"),
        Color::Rgb(r, g, b) => {
            out.push_str("\x1b[38;2;");
            push_u16(out, r as u16);
            out.push(';');
            push_u16(out, g as u16);
            out.push(';');
            push_u16(out, b as u16);
            out.push('m');
        }
        Color::Indexed(n) => {
            out.push_str("\x1b[38;5;");
            push_u16(out, n as u16);
            out.push('m');
        }
    }
}

fn push_bg_color(out: &mut String, color: Color) {
    match color {
        Color::Reset => out.push_str("\x1b[49m"),
        Color::Black => out.push_str("\x1b[40m"),
        Color::Red => out.push_str("\x1b[41m"),
        Color::Green => out.push_str("\x1b[42m"),
        Color::Yellow => out.push_str("\x1b[43m"),
        Color::Blue => out.push_str("\x1b[44m"),
        Color::Magenta => out.push_str("\x1b[45m"),
        Color::Cyan => out.push_str("\x1b[46m"),
        Color::Gray => out.push_str("\x1b[47m"),
        Color::DarkGray => out.push_str("\x1b[100m"),
        Color::LightRed => out.push_str("\x1b[101m"),
        Color::LightGreen => out.push_str("\x1b[102m"),
        Color::LightYellow => out.push_str("\x1b[103m"),
        Color::LightBlue => out.push_str("\x1b[104m"),
        Color::LightMagenta => out.push_str("\x1b[105m"),
        Color::LightCyan => out.push_str("\x1b[106m"),
        Color::White => out.push_str("\x1b[107m"),
        Color::Rgb(r, g, b) => {
            out.push_str("\x1b[48;2;");
            push_u16(out, r as u16);
            out.push(';');
            push_u16(out, g as u16);
            out.push(';');
            push_u16(out, b as u16);
            out.push('m');
        }
        Color::Indexed(n) => {
            out.push_str("\x1b[48;5;");
            push_u16(out, n as u16);
            out.push('m');
        }
    }
}

impl Backend for WebBackend {
    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        for (x, y, cell) in content {
            if x < self.width && y < self.height {
                let idx = usize::from(y) * usize::from(self.width) + usize::from(x);
                self.cells[idx] = cell.clone();
            }
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.cursor_visible = false;
        Ok(())
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.cursor_visible = true;
        Ok(())
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        Ok(Position::new(self.cursor_x, self.cursor_y))
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let pos = position.into();
        self.cursor_x = pos.x;
        self.cursor_y = pos.y;
        Ok(())
    }

    fn clear(&mut self) -> io::Result<()> {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
        Ok(())
    }

    fn size(&self) -> io::Result<Size> {
        Ok(Size::new(self.width, self.height))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        Ok(WindowSize {
            columns_rows: Size::new(self.width, self.height),
            pixels: Size::default(),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.ansi_output = self.render_to_ansi();
        Ok(())
    }
}
