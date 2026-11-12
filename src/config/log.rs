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
    fn a_line_with_nothing_to_hide_is_written_as_it_is() {
        let line = logged(|| tracing::info!("queue: 12 tracks"));

        assert!(line.contains("queue: 12 tracks"), "{line}");
    }
}
