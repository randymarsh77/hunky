use std::cell::RefCell;

thread_local! {
    static ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn debug(message: String) {
    if message.starts_with("Failed") {
        ERROR.with(|error| *error.borrow_mut() = Some(message));
    }
}

pub fn take_error() -> Option<String> {
    ERROR.with(|error| error.borrow_mut().take())
}
