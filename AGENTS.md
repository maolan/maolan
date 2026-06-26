# Agent Instructions: `daw`

## End-of-Change Routine

After every code change in this directory, run these commands in order:

```bash
cargo clippy --all-targets --fix --allow-dirty
cargo fmt
```

## Clippy Warnings

If `cargo clippy --all-targets --fix --allow-dirty` does **not** automatically fix all warnings, fix the remaining warnings manually.

- Do **not** use `#![allow(...)]` or `#[allow(...)]` directives to silence clippy warnings.
- Address the underlying issue reported by clippy.

Always ensure both commands complete successfully with no remaining warnings before finishing.
