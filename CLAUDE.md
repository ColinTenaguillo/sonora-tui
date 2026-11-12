# sonora-tui

Read CONTRIBUTING.md before touching the build, the tests or a release; this
file only lists what goes wrong when it is skipped.

## Checks (what CI runs, on Linux, macOS and Windows)

    cargo build --locked --all-targets
    cargo test --locked
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings

Start with the narrowest test (`cargo test --lib <name>`, `cargo test --test
keyboard`). `tests/mpris` runs on Linux only. A failing test or clippy warning is yours
until shown otherwise: none are known to be broken on `main`. Run `cargo fmt`
before committing; the tree is rustfmt's and CI checks it.

## Traps

- Never run a plain `cargo update`: it re-resolves the macOS media-key pins
  and breaks that build. Bump one crate with `--precise` (see CONTRIBUTING).
- `music`, `storage` and `webview` are git pins to sonorahq/sonora, moved
  weekly by `update-music.yml`. What they lack is proposed upstream first;
  when Sonora declines it, as it did for playlist lengths, it is done here.
- No `unwrap` outside tests and `main`, and no component reaching into
  another's internals: `tests/architecture.rs` fails the build on either.
- On macOS `main` is not `#[tokio::main]`: the main thread runs the media
  keys' run loop.
- Every service, TIDAL included, is sonora's provider. What one has that
  sonora's `MusicApi` lacks goes behind `catalogue::Extras`, in one adapter
  file per service (`tidal_extras.rs`), written to be deleted the day
  sonora covers it: nothing else may speak to a service's API. Its DTO
  fields are `#[serde(default)]`, so a misspelt one parses to nothing: a
  new field is tested on a body copied from a real answer.
- `cargo test` must never touch the user's own files;
  `catalogue::carry_tidal_sign_in_over` moves one and is called from `main`
  alone.
- A new key goes in `src/shell/help.rs` (tests check it) and in the README's
  key table (nothing checks it). A new config key goes in the README too.

## Tests

Test what is drawn: press keys, read the buffer. Then break the line the test
covers and watch it fail; several tests here once asserted nothing.

To run the app itself, use tmux with a throwaway `HOME`, a folder of local
files and `volume = 0.02`. Never confirm a TIDAL or Spotify sign-in from a
test: it opens a browser tab on the owner's machine.

## Writing

Comments say why. Write for someone reading the code at `main` who never saw
the conversation that produced it; none of it goes into comments, commits or
pull requests.

## How changes reach `main`

`main` is protected: a pull request merges once the three CI jobs pass, and
only by rebase, since the commit subjects are the changelog `cog` prints.
Code goes through a pull request with the commit subject as its title and
the body as its description. `chore(version)` from `cog bump`, and `docs`
or `chore` that touch no code, may be pushed straight to `main`.
