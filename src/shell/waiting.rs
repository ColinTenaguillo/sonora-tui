//! The spinner on a message that waits: a dot sliding back and forth
//! between two faint ones, `●··`, `·●·`, `··●`, `·●·`, as lazygit draws
//! its own beside "Pushing". The frame comes from the clock rather than
//! from state: the screen is redrawn on every tick, so a message that
//! stands long enough to be read moves every eighth of a second, and a
//! test sees one of the frames.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::Instant;

const FRAMES: [&str; 4] = ["●··", "·●·", "··●", "·●·"];
const STEP_MS: u128 = 125;

/// The frame for now.
pub fn spinner() -> &'static str {
    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed().as_millis();
    FRAMES[((elapsed / STEP_MS) % FRAMES.len() as u128) as usize]
}

/// `text` with a trailing ellipsis replaced by the spinner; anything else
/// untouched, so a finished message is drawn as written.
pub fn animated(text: &str) -> Cow<'_, str> {
    match text.strip_suffix('…') {
        Some(head) => format!("{head} {}", spinner()).into(),
        None => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_ellipsis_spins() {
        let moving = animated("Searching…");
        assert!(moving.starts_with("Searching "), "{moving}");
        assert!(FRAMES.iter().any(|f| moving.ends_with(f)), "{moving}");
        assert_eq!(animated("added to Road"), "added to Road");
    }
}
