//! Playing audio: the queue, the listening history and the engine thread.
//!
//! The submodules are `pub(crate)`: the facade below is the whole public
//! surface, so nothing outside can couple to the engine's internals.

pub(crate) mod engine;
pub(crate) mod history;
pub(crate) mod queue;

pub use engine::{spawn, Bands, Cmd, Options, PlaybackEvent};
pub use history::{load_history, save_history};
pub use queue::{clock_rng, Queue, Repeat, Source};
