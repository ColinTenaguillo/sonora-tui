//! What native code writes to stderr while the TUI is up is read line by
//! line instead of landing across the screen, and the terminal gets stderr
//! back after.
//!
//! A binary of its own: fd 2 is the whole process's, and no other test may
//! be writing to it while this one moves it.
#![cfg(unix)]

use std::sync::mpsc;
use std::time::Duration;

/// Which open file fd 2 is now.
fn stderr_is() -> (u64, u64) {
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::fstat(2, &mut stat) }, 0, "fd 2 is open");
    (stat.st_dev as u64, stat.st_ino as u64)
}

/// Writes past Rust's std, as GTK or GStreamer would.
fn native_write(text: &str) -> isize {
    unsafe { libc::write(2, text.as_ptr().cast(), text.len()) }
}

#[test]
fn native_stderr_is_read_by_line_until_restored() {
    let terminal = stderr_is();
    let (lines, read) = mpsc::channel();

    let diverted = sonora_tui::config::log::divert_stderr(move |line| {
        let _ = lines.send(line);
    })
    .unwrap();
    let moved = stderr_is();
    let text = "Could not determine type of stream.\nsecond\n";
    let written = native_write(text);
    diverted.restore();

    assert_ne!(moved, terminal, "stderr was not moved off the terminal");
    assert_eq!(stderr_is(), terminal, "stderr was not given back");
    assert_eq!(written, text.len() as isize);
    let next = || read.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(next(), "Could not determine type of stream.");
    assert_eq!(next(), "second");
}
