//! A desktop notification when the track changes, with its cover where the
//! system takes one.
//!
//! The system wants the picture as a file, so the cover is fetched again
//! here and written to the temporary folder: the covers the shell draws are
//! decoded for the terminal and kept in memory only. One file, written over
//! at each track, and one notification, replaced where the system allows it,
//! so skipping through an album does not leave a pile of either.

use std::path::{Path, PathBuf};

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

/// The track that just started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Where its cover is: an address, or `file://` and a path.
    pub cover: Option<String>,
}

impl Notice {
    /// The line under the title: who, and on what. A single is an album
    /// named like its track: saying it twice wraps the line, and on macOS
    /// the taller banner leaves the cover off centre.
    fn body(&self) -> String {
        [self.artist.as_str(), self.album.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty() && !part.eq_ignore_ascii_case(&self.title))
            .collect::<Vec<_>>()
            .join(" \u{2014} ")
    }
}

/// Starts the task that shows what it is sent. Dropping the sender ends it.
pub fn spawn() -> UnboundedSender<Notice> {
    let (sender, mut receiver) = unbounded_channel::<Notice>();
    tokio::spawn(async move {
        let http = crate::config::http();
        while let Some(mut notice) = receiver.recv().await {
            // Only the latest matters: a burst of skips is one notification.
            while let Ok(newer) = receiver.try_recv() {
                notice = newer;
            }
            let picture = match &notice.cover {
                Some(cover) if takes_pictures() => picture_of(&http, cover).await,
                _ => None,
            };
            // The system's call blocks, on a bus or on the window server.
            let shown = tokio::task::spawn_blocking(move || show(&notice, picture.as_deref()));
            if let Ok(Err(error)) = shown.await {
                tracing::info!("notification not shown: {error}");
            }
        }
    });
    sender
}

/// The cover as a file the system can read, or nothing when it cannot be
/// had: the notification is then shown without it.
async fn picture_of(http: &reqwest::Client, cover: &str) -> Option<PathBuf> {
    if let Some(path) = cover.strip_prefix("file://") {
        return Some(PathBuf::from(path));
    }
    let bytes = http.get(cover).send().await.ok()?.bytes().await.ok()?;
    let path = picture_path(&bytes)?;
    tokio::fs::write(&path, &bytes).await.ok()?;
    Some(path)
}

/// Where the picture is written: named for this process, so two of them do
/// not write over each other, and with the extension its bytes call for,
/// which Windows reads the format from.
fn picture_path(bytes: &[u8]) -> Option<PathBuf> {
    let extension = image::guess_format(bytes).ok()?.extensions_str().first()?;
    Some(std::env::temp_dir().join(format!(
        "sonora-tui-cover-{}.{extension}",
        std::process::id()
    )))
}

/// Set once `terminal-notifier` was looked for and is not installed: the
/// cover is then not fetched on macOS, where nothing else can show it.
static NO_NOTIFIER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the notification can carry a picture here.
fn takes_pictures() -> bool {
    !cfg!(target_os = "macos") || !NO_NOTIFIER.load(std::sync::atomic::Ordering::Relaxed)
}

/// macOS accepts a notification from a program that is not an application
/// bundle and then shows nothing: the call notify-rust makes there returned
/// success on macOS 27 and no banner came. So it goes through one of two
/// programs that are: `terminal-notifier` when it is installed, which takes
/// the cover and replaces its last notification, and AppleScript otherwise,
/// shown under Script Editor's name with no picture.
#[cfg(target_os = "macos")]
fn show(notice: &Notice, picture: Option<&Path>) -> Result<(), String> {
    match through_terminal_notifier(notice, picture) {
        Ok(()) => Ok(()),
        Err(why) => {
            tracing::debug!("terminal-notifier: {why}; AppleScript instead");
            through_applescript(notice)
        }
    }
}

#[cfg(target_os = "macos")]
fn through_terminal_notifier(notice: &Notice, picture: Option<&Path>) -> Result<(), String> {
    let mut command = std::process::Command::new("terminal-notifier");
    command
        .args(["-title", &for_terminal_notifier(&notice.title)])
        .args(["-message", &for_terminal_notifier(&notice.body())])
        // One group, so each notification takes the place of the last.
        .args(["-group", "sonora-tui"]);
    if let Some(path) = picture {
        command.arg("-contentImage").arg(path);
    }
    // It answers on stderr when the system refuses it; the terminal is the
    // app's, so nothing of it may be written there.
    let output = command.output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            NO_NOTIFIER.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        error.to_string()
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// A text as `terminal-notifier` takes it: it refuses one that opens with a
/// bracket, a quote or a dash ("Could not read the -message value") unless
/// a backslash is first, which it takes off again.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn for_terminal_notifier(text: &str) -> String {
    match text.chars().next() {
        Some('-' | '[' | '(' | '{' | '<' | '"' | '\'') => format!("\\{text}"),
        _ => text.to_owned(),
    }
}

#[cfg(target_os = "macos")]
fn through_applescript(notice: &Notice) -> Result<(), String> {
    // The texts are arguments rather than spliced into the script, so a
    // title with a quote in it is still a title.
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 2 of argv) with title (item 1 of argv)",
            "-e",
            "end run",
            &notice.title,
            &notice.body(),
        ])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "osascript: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(not(target_os = "macos"))]
fn show(notice: &Notice, picture: Option<&Path>) -> Result<(), String> {
    let mut notification = notify_rust::Notification::new();
    notification
        .appname("sonora-tui")
        .summary(&notice.title)
        .body(&notice.body());
    if let Some(path) = picture.and_then(Path::to_str) {
        notification.image_path(path);
    }
    replacing_the_last(&mut notification).map_err(|error| error.to_string())
}

/// On the desktops that speak the notification bus, each one takes the
/// place of the one before, by the id the server gave it; and it is marked
/// transient, so the history keeps none of them.
#[cfg(all(unix, not(target_os = "macos")))]
fn replacing_the_last(
    notification: &mut notify_rust::Notification,
) -> notify_rust::error::Result<()> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static LAST: AtomicU32 = AtomicU32::new(0);

    notification.hint(notify_rust::Hint::Transient(true));
    match LAST.load(Ordering::Relaxed) {
        0 => {}
        last => {
            notification.id(last);
        }
    }
    let handle = notification.show()?;
    LAST.store(handle.id(), Ordering::Relaxed);
    Ok(())
}

/// Windows stacks its own, and hands back no id to replace.
#[cfg(windows)]
fn replacing_the_last(
    notification: &mut notify_rust::Notification,
) -> notify_rust::error::Result<()> {
    notification.show().map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(artist: &str, album: &str) -> Notice {
        Notice {
            title: "Song".into(),
            artist: artist.into(),
            album: album.into(),
            cover: None,
        }
    }

    #[test]
    fn the_body_names_the_artist_and_the_album_and_leaves_out_what_is_empty() {
        assert_eq!(notice("Someone", "Album").body(), "Someone \u{2014} Album");
        assert_eq!(notice("Someone", "").body(), "Someone");
        assert_eq!(notice("Someone", "SONG").body(), "Someone", "a single");
        assert_eq!(notice("", "").body(), "");
    }

    #[test]
    fn the_picture_is_named_for_the_format_its_bytes_are_in() {
        let png = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let jpeg = [0xff, 0xd8, 0xff, 0xe0];
        let named = |bytes: &[u8]| {
            picture_path(bytes).and_then(|path| {
                path.extension()
                    .map(|extension| extension.to_string_lossy().into_owned())
            })
        };
        assert_eq!(named(&png).as_deref(), Some("png"));
        assert_eq!(named(&jpeg).as_deref(), Some("jpg"));
        assert_eq!(named(b"<html>not a picture"), None);
    }

    #[test]
    fn a_text_terminal_notifier_would_refuse_is_escaped_and_the_rest_left_alone() {
        assert_eq!(for_terminal_notifier("[Live] Song"), "\\[Live] Song");
        assert_eq!(for_terminal_notifier("-1"), "\\-1");
        assert_eq!(
            for_terminal_notifier("(What's the Story)"),
            "\\(What's the Story)"
        );
        assert_eq!(for_terminal_notifier("\"Heroes\""), "\\\"Heroes\"");
        assert_eq!(for_terminal_notifier("Song [Live]"), "Song [Live]");
        assert_eq!(for_terminal_notifier(""), "");
    }

    #[tokio::test]
    async fn a_cover_already_on_disk_is_handed_over_where_it_is() {
        let http = crate::config::http();
        assert_eq!(
            picture_of(&http, "file:///music/album/cover.jpg").await,
            Some(PathBuf::from("/music/album/cover.jpg"))
        );
    }
}
