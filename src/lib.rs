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
pub mod playback;
pub mod presence;
pub mod scrobble;
pub mod search;
pub mod shell;
