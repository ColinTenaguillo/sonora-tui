//! sonora-tui — a terminal music player over sonora's services.
//!
//! Modules are business capabilities, each exposing a facade while its
//! submodules stay `pub(crate)`. `domain` is the dependency-free core that
//! everything else points at.

pub mod browse;
pub mod catalogue;
pub mod config;
pub mod domain;
pub mod library;
pub mod notify;
pub mod playback;
pub mod presence;
pub mod scrobble;
pub mod search;
pub mod shell;

/// Every test runs in a home folder of its own, named before the first
/// test thread starts, so the environment is written while nothing reads
/// it. Every folder sonora and this app keep follows `HOME` once the XDG
/// variables are gone. Windows names its folders by an API no variable
/// moves, so there the tests still see the user's own.
#[cfg(all(test, unix))]
#[ctor::ctor(unsafe)]
fn tests_away_from_home() {
    let home = std::env::temp_dir().join(format!("sonora-tui-tests-{}", std::process::id()));
    // Emptied first: a run that wrote there and was given the same pid
    // would hand this one its config, cache or history.
    let _ = std::fs::remove_dir_all(&home);
    std::env::set_var("HOME", home);
    for xdg in [
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
    ] {
        std::env::remove_var(xdg);
    }
}

#[cfg(test)]
mod tests {
    /// The first time a provider names its accounts folder, sonora moves
    /// the sign-ins an older Sonora left in the cache, and the first time
    /// the app names its own folders it moves ratidal's: a test that
    /// reached either on the user's home folder would move the user's files.
    #[test]
    #[cfg(unix)]
    fn the_tests_run_in_a_home_folder_of_their_own() {
        let home = std::env::home_dir().expect("a home");
        assert!(home.starts_with(std::env::temp_dir()), "{}", home.display());
        // sonora's cache, where the sign-ins are moved from; the data
        // folder, where they are moved to and the app kept its token; the
        // app's cache and config.
        let folders = [
            Some(music::credentials::root()),
            crate::config::paths::legacy_token_file(),
            crate::config::paths::cache_dir(),
            crate::config::paths::config_file(),
        ];
        for folder in folders {
            let folder = folder.expect("a folder");
            assert!(folder.starts_with(&home), "{}", folder.display());
        }
    }
}
