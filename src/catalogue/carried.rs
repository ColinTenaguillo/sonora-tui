//! The TIDAL sign-in this app kept itself, before TIDAL came through
//! sonora: carried over to where sonora keeps its own, once, so an update
//! does not sign anyone out.

use std::path::{Path, PathBuf};

/// The sign-in as sonora reads it. The two files hold the same fields
/// under the same names, but for the account's id, a number here and text
/// there. `None` for a file that is no sign-in at all.
fn as_sonora_keeps_it(ours: &str) -> Option<String> {
    let mut kept: serde_json::Value = serde_json::from_str(ours).ok()?;
    let fields = kept.as_object_mut()?;
    fields
        .get("refresh_token")?
        .as_str()
        .filter(|t| !t.is_empty())?;
    let id = fields.get("user_id")?.as_u64()?.to_string();
    fields.insert("user_id".into(), id.into());
    Some(kept.to_string())
}

/// Where sonora's TIDAL provider keeps its sign-in.
pub(super) fn sonoras_file() -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    Some(
        dirs.data_local_dir()
            .join("sonora/accounts/tidal/credentials.json"),
    )
}

/// Puts `contents` at `path`, readable by the owner alone as sonora writes
/// its own, through a file beside it that is renamed over: a write cut
/// short leaves no half of a sign-in for sonora to find.
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let beside = path.with_extension("json.carried");
    let _ = std::fs::remove_file(&beside);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options.open(&beside).and_then(|mut file| {
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    });
    let placed = written.and_then(|()| std::fs::rename(&beside, path));
    if placed.is_err() {
        let _ = std::fs::remove_file(&beside);
    }
    placed
}

/// Whether the file at `a` was written after the one at `b`. Not when
/// either cannot say.
fn newer(a: &Path, b: &Path) -> bool {
    let written = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    matches!((written(a), written(b)), (Some(a), Some(b)) if a > b)
}

/// Moves the first sign-in among `ours` to `theirs`. One sonora already
/// has there stays unless ours is the newer, which is sonora's own rule
/// for a sign-in it finds in two places. Every file of ours goes once its
/// sign-in is settled: nothing reads it again, and a token is not a thing
/// to leave lying.
fn carry(ours: &[PathBuf], theirs: &Path) {
    for path in ours {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let wanted = !theirs.exists() || newer(path, theirs);
        if let (true, Some(kept)) = (wanted, as_sonora_keeps_it(&text)) {
            match write_private(theirs, &kept) {
                Ok(()) => tracing::info!("carried the TIDAL sign-in over to sonora's"),
                // Kept for the next start rather than lost: signing in
                // again is the worst that happens without it.
                Err(error) => {
                    tracing::warn!("could not carry the TIDAL sign-in over: {error}");
                    continue;
                }
            }
        }
        match std::fs::remove_file(path) {
            Ok(()) => tracing::info!("removed {}, which nothing reads now", path.display()),
            Err(error) => tracing::warn!("could not remove {}: {error}", path.display()),
        }
    }
}

/// Run once as the program starts, before any provider looks for its
/// sign-in, and from there alone: it moves the user's own files. Nothing
/// to do, and nothing done, from the second start on.
pub fn tidal_sign_in() {
    let ours: Vec<PathBuf> = [
        crate::config::paths::token_file(),
        crate::config::paths::legacy_token_file(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(theirs) = sonoras_file() {
        carry(&ours, &theirs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = r#"{"access_token":"a","refresh_token":"r","expires_at":1700000000,
        "country_code":"FR","user_id":123456,"lane":"app"}"#;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sonora-tui-carried-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_sign_in_reads_as_sonoras_with_the_account_id_as_text() {
        let kept: serde_json::Value =
            serde_json::from_str(&as_sonora_keeps_it(OURS).unwrap()).unwrap();
        assert_eq!(kept["user_id"], "123456");
        for (field, value) in [
            ("access_token", "a"),
            ("refresh_token", "r"),
            ("country_code", "FR"),
            ("lane", "app"),
        ] {
            assert_eq!(kept[field], value, "{field}");
        }
        assert_eq!(kept["expires_at"], 1_700_000_000u64);
    }

    #[test]
    fn what_is_no_sign_in_is_not_carried() {
        assert_eq!(as_sonora_keeps_it("not json"), None);
        assert_eq!(as_sonora_keeps_it(r#"{"user_id":1}"#), None);
        assert_eq!(
            as_sonora_keeps_it(r#"{"refresh_token":"","user_id":1}"#),
            None,
            "signed out"
        );
    }

    #[test]
    fn our_file_moves_to_sonoras_place_and_is_gone() {
        let dir = dir("moves");
        let (ours, theirs) = (dir.join("token.json"), dir.join("accounts/tidal/c.json"));
        std::fs::write(&ours, OURS).unwrap();

        carry(&[dir.join("absent.json"), ours.clone()], &theirs);

        assert!(!ours.exists(), "nothing reads it again");
        let kept = std::fs::read_to_string(&theirs).unwrap();
        assert!(kept.contains(r#""user_id":"123456""#), "{kept}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&theirs).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "the owner's alone");
        }
    }

    #[test]
    fn a_sign_in_sonora_already_has_is_left_as_it_is() {
        let dir = dir("kept");
        let (ours, theirs) = (dir.join("token.json"), dir.join("c.json"));
        std::fs::write(&ours, OURS).unwrap();
        std::fs::write(&theirs, "theirs").unwrap();
        age(&ours, 60);

        carry(std::slice::from_ref(&ours), &theirs);

        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), "theirs");
        assert!(!ours.exists(), "ours goes all the same");
    }

    /// Dates the file `seconds` back.
    fn age(path: &Path, seconds: u64) {
        let then = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(then).unwrap();
    }

    #[test]
    fn ours_replaces_an_older_one_of_sonoras_whole() {
        // Signed in to the desktop app long ago, and here since.
        let dir = dir("newer");
        let (ours, theirs) = (dir.join("token.json"), dir.join("c.json"));
        std::fs::write(
            &theirs,
            "theirs, and longer than what replaces it".repeat(20),
        )
        .unwrap();
        age(&theirs, 3_600);
        std::fs::write(&ours, OURS).unwrap();

        carry(std::slice::from_ref(&ours), &theirs);

        let kept: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&theirs).unwrap()).unwrap();
        assert_eq!(kept["refresh_token"], "r", "nothing of the old one is left");
        assert!(!ours.exists());
        assert!(!dir.join("c.json.carried").exists(), "nor of the write");
    }
}
