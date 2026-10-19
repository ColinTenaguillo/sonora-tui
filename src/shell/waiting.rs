//! The animated ellipsis on a message that waits: "Loading.", "Loading..",
//! "Loading...", as lazygit draws its own. The frame comes from the clock
//! rather than from state: the screen is redrawn on every tick, so a
//! message that stands long enough to be read moves a dot every third of
//! a second, and a test sees one of the three forms.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::Instant;

const STEP_MS: u128 = 350;

/// One to three dots, by the clock.
pub fn dots() -> &'static str {
    static START: OnceLock<Instant> = OnceLock::new();
    let elapsed = START.get_or_init(Instant::now).elapsed().as_millis();
    match (elapsed / STEP_MS) % 3 {
        0 => ".",
        1 => "..",
        _ => "...",
    }
}

/// `text` with a trailing ellipsis animated; anything else untouched, so
/// a finished message is drawn as written.
pub fn animated(text: &str) -> Cow<'_, str> {
    match text.strip_suffix('…') {
        Some(head) => format!("{head}{}", dots()).into(),
        None => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_ellipsis_moves() {
        let moving = animated("Searching…");
        assert!(moving.starts_with("Searching."), "{moving}");
        assert!(moving.len() <= "Searching...".len());
        assert_eq!(animated("added to Road"), "added to Road");
    }
}
