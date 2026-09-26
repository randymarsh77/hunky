#[cfg(not(feature = "browser"))]
pub use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(feature = "browser")]
pub use portable::*;

#[cfg(feature = "browser")]
mod portable {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum KeyCode {
        Char(char),
        Up,
        Down,
        Enter,
        Esc,
        Tab,
        BackTab,
    }

    #[derive(Clone, Copy, Debug, Default)]
    pub struct KeyModifiers(pub u8);

    impl KeyModifiers {
        pub const SHIFT: Self = Self(1);
        pub const CONTROL: Self = Self(2);

        pub fn contains(self, other: Self) -> bool {
            self.0 & other.0 == other.0
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct KeyEvent {
        pub code: KeyCode,
        pub modifiers: KeyModifiers,
    }
}
