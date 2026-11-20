//! What the log file is written through.

use std::io::{self, Write};

use tracing::Subscriber;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

/// What is logged when `RUST_LOG` does not say. sonora's crates are in it
/// at `warn`: the engine tells the shell only that a track could not play,
/// and writes why here.
const DEFAULT_FILTER: &str = "sonora_tui=info,music=warn,storage=warn,webview=warn,rodio=warn";

/// The subscriber that writes the log to `file`, to be installed by `main`.
pub fn to<M>(file: M) -> impl Subscriber + Send + Sync
where
    M: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    tracing_subscriber::fmt()
        .with_writer(Redacted(file))
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| DEFAULT_FILTER.into()),
        )
        .finish()
}

/// A log writer that passes each record through `music::redact`: an address
/// loses its user info, query and fragment, the name after `user:` or
/// `/user/` is hidden and the home folder reads as `~`. The `music` crate
/// logs those as they are and leaves the hiding to whoever owns the log, and
/// the file gets attached to bug reports. A name or a secret written as
/// plain words is not recognised and still gets through.
struct Redacted<W>(W);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Redacted<M> {
    type Writer = Redacted<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        Redacted(self.0.make_writer())
    }
}

impl<W: Write> Write for Redacted<W> {
    /// The formatter hands over a whole record at a time, so an address is
    /// never split across two calls.
    fn write(&mut self, record: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(record);
        self.0.write_all(music::redact(&text).as_bytes())?;
        Ok(record.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Takes over stderr while the TUI owns the terminal: each line written to
/// it is handed to `line`, and what comes back points it at the terminal
/// again. Libraries write warnings straight to fd 2, which is the terminal
/// the TUI draws on, so their lines landed across the screen; GTK,
/// WebKitGTK and GStreamer do it in the Linux sign-in window and the web
/// processes it starts, which inherit fd 2 and keep writing to the pipe.
#[cfg(unix)]
pub fn divert_stderr(mut line: impl FnMut(String) + Send + 'static) -> io::Result<Diverted> {
    use std::io::BufRead;
    use std::os::fd::{AsFd, AsRawFd, IntoRawFd};
    // Close-on-exec, unlike `dup`: a child the app starts gets no handle on
    // the terminal through it. Kept open for as long as the process lives.
    let terminal = io::stderr().as_fd().try_clone_to_owned()?.into_raw_fd();
    let (reader, writer) = io::pipe()?;
    if unsafe { libc::dup2(writer.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(io::Error::last_os_error());
    }
    std::thread::Builder::new()
        .name("stderr".into())
        .spawn(move || {
            for text in io::BufReader::new(reader)
                .split(b'\n')
                .map_while(Result::ok)
            {
                line(String::from_utf8_lossy(&text).into_owned());
            }
        })?;
    Ok(Diverted { terminal })
}

/// Logs a line a library wrote to stderr, as a warning, under the app's own
/// target so the default filter keeps it. The redaction applies: such a line
/// can name a stream's signed address or a path under the home folder.
pub fn from_stderr(line: String) {
    tracing::warn!(target: "sonora_tui::stderr", "{line}");
}

/// Elsewhere stderr stays where it is: nothing there is known to write to it.
#[cfg(not(unix))]
pub fn divert_stderr(_line: impl FnMut(String) + Send + 'static) -> io::Result<Diverted> {
    Ok(Diverted {})
}

/// The terminal's stderr, kept open while fd 2 points at the pipe.
#[derive(Clone, Copy)]
pub struct Diverted {
    #[cfg(unix)]
    terminal: std::os::raw::c_int,
}

impl Diverted {
    /// Points stderr at the terminal again, for what is printed once the
    /// TUI is gone: a panic's message, or the error the app ends on. Calling
    /// it twice does no harm.
    pub fn restore(self) {
        #[cfg(unix)]
        unsafe {
            libc::dup2(self.terminal, libc::STDERR_FILENO)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, record: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(record)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn logged(emit: impl FnOnce()) -> String {
        let file = Arc::new(Mutex::new(Vec::new()));
        let sink = file.clone();
        tracing::subscriber::with_default(to(move || Sink(sink.clone())), emit);
        let written = file.lock().unwrap().clone();
        String::from_utf8(written).unwrap()
    }

    #[test]
    fn a_logged_account_name_and_token_do_not_reach_the_file() {
        let line = logged(|| {
            tracing::warn!(
                "play in spotify:user:colin:collection from https://api.example/v1/me?token=s3cret"
            )
        });

        assert!(line.contains("spotify:user:<user>:collection"), "{line}");
        assert!(line.contains("https://api.example/v1/me?"), "{line}");
        assert!(!line.contains("colin"), "{line}");
        assert!(!line.contains("s3cret"), "{line}");
    }

    #[test]
    fn a_library_line_from_stderr_is_kept_and_redacted() {
        let line = logged(|| {
            from_stderr("Could not open resource https://cdn.example/a.flac?token=s3cret".into())
        });

        assert!(
            line.contains("Could not open resource https://cdn.example/a.flac?"),
            "{line}"
        );
        assert!(!line.contains("s3cret"), "{line}");
    }

    #[test]
    fn a_line_with_nothing_to_hide_is_written_as_it_is() {
        let line = logged(|| tracing::info!("queue: 12 tracks"));

        assert!(line.contains("queue: 12 tracks"), "{line}");
    }
}
