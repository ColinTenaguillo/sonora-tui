# sonora-tui

Read CONTRIBUTING.md before touching the build, the tests or a release; this
file only lists what goes wrong when it is skipped.

## Checks (what CI runs, on Linux, macOS and Windows)

    cargo build --locked --all-targets
    cargo test --locked
    cargo fmt --check
    cargo clippy --locked --all-targets -- -D warnings

Start with the narrowest test (`cargo test --lib <name>`, `cargo test --test
keyboard`). `tests/live_*` are `#[ignore]`d and need a signed-in account;
`tests/mpris` runs on Linux only. A failing test or clippy warning is yours
until shown otherwise: none are known to be broken on `main`. Run `cargo fmt`
before committing; the tree is rustfmt's and CI checks it.

## Traps

- Never run a plain `cargo update`: it re-resolves the macOS media-key pins
  and breaks that build. Bump one crate with `--precise` (see CONTRIBUTING).
- `music`, `storage` and `webview` are git pins to sonorahq/sonora, moved
  weekly by `update-music.yml`. What they lack is fixed upstream with a pull
  request there, not worked around here.
- No `unwrap` outside tests and `main`, and no component reaching into
  another's internals: `tests/architecture.rs` fails the build on either.
- On macOS `main` is not `#[tokio::main]`: the main thread runs the media
  keys' run loop.
- DTO fields are `#[serde(default)]`, so a misspelt field parses to nothing.
  A new field gets a check in `tests/live_api.rs`.
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
