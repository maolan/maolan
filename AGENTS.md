# Repository Guidelines

## Project Structure & Module Organization
`src/` contains the desktop DAW application, including GUI state,
workspace views, widgets, plugin integration, and platform-specific app
code. `engine/src/` contains the `maolan-engine` crate for audio, MIDI,
routing, hardware backends, workers, and plugin hosting. Reference docs
live in `docs/`, images for the README live in `images/`, and
CI/release workflows live in `.github/workflows/`.

Tests are mostly inline Rust unit tests placed beside the code they
validate, for example in `src/gui/mod.rs` and `engine/src/history.rs`.
Build artifacts go to `target/` and `engine/target/`; do not commit
them.

## Build, Test, and Development Commands
Use Cargo from the repository root:

```bash
cargo build
cargo test
cargo test -p maolan-engine
cargo run --release
cargo run --release -- --debug
```

`cargo build` checks the workspace locally. `cargo test` runs the root
crate tests. `cargo test -p maolan-engine` runs the engine crate tests
used by CI. Run both test commands for verification. `cargo run --release`
launches the app, and `-- --debug` enables extra runtime logging. On
Linux and FreeBSD, install the native packages used in CI first,
especially JACK, ALSA/X11, Lilv, Suil, and GTK2 development libraries.

## Coding Style & Naming Conventions
Follow standard Rust formatting with 4-space indentation and run
`cargo fmt` before submitting changes. Keep module and function names
`snake_case`, types and enums `CamelCase`, and constants
`SCREAMING_SNAKE_CASE`. Match the existing pattern of colocating UI
logic under `src/gui/`, reusable widgets under `src/widget/`, and
engine internals under the closest `engine/src/*` module.

Prefer small focused modules over broad utility files. Keep
platform-specific code explicit, using `cfg` gates in the same style as
the existing ALSA, OSS, CoreAudio, and LV2/VST3 modules.

## Testing Guidelines
Add unit tests next to changed code with `#[cfg(test)]` modules and
descriptive test names such as `restores_automation_after_undo`. Run
both `cargo test` and `cargo test -p maolan-engine` before opening a
PR. During development, always run `cargo fmt` and `cargo clippy` after
code changes. Run both test commands when the feature slice is
complete, not on every intermediate step. Treat this as a hard
requirement. When touching platform or
plugin-hosting paths that are difficult to exercise locally, add
coverage for the pure logic around them and call out any untested edges
in the PR.

Write code so it is testable as a hard requirement, not an aspiration.
Prefer designs that isolate pure logic from UI, IO, threading, and
platform glue so behavior can be covered with unit tests. When a change
lands in a path that is awkward to test directly, extract the decision
logic into small helpers or focused modules and test those instead of
leaving the behavior embedded in event handlers or side-effect-heavy
code.

## Commit & Pull Request Guidelines
Recent history favors short imperative commit subjects, often
capitalized, for example `Update dependencies` or `Simplify tempo
strip`. Keep the subject line concise and mention the user-visible
behavior when possible.

Pull requests should explain the behavior change, note platform impact,
and link the relevant issue if one exists. Include screenshots or GIFs
for visible UI changes, and mention any required system libraries or
manual verification steps when audio or plugin behavior is involved.

Always run `cargo clippy` for Rust verification when Clippy is
available. Do not skip it, and do not use `cargo check` as a substitute
unless Clippy is unavailable or the user explicitly asks for
`cargo check`. If Clippy cannot be run, state why. Format Rust code by
running `cargo fmt` (not `cargo fmt --check`). This is mandatory after
code changes, even for intermediate feature slices. Running
`cargo test -p maolan-engine` whenever tests are run is also mandatory.
