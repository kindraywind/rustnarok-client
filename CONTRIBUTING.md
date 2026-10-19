# Contributing

Thanks for looking. rustnarok-client is an alpha, and issues and pull requests are welcome.

## Before you start

- Open an issue (or pick an [open one](https://github.com/kindraywind/rustnarok-client/issues)) for anything bigger than a
  small fix, so we can agree on it first.
- **Never commit game data**: no `.grf`, nothing extracted from one, no screenshots of the game. Fact tables written by
  hand (`effects.txt`, `status_icons.txt`, `packet_lengths.txt`) are code. CI and review will turn such files down.
- Protocol facts come from [rathena](https://github.com/rathena/rathena)'s source.

## Building and checking

You need a nightly Rust (`rust-toolchain.toml` pins it) and, to play, your own official `data.grf`. See the
[README](README.md#-quick-start).

```bash
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release
```

Tests that read `data.grf` skip themselves when it is missing, as in CI. `.claude/skills/rustnarok-dev/verify.sh` runs
the same checks locally. [AGENTS.md](AGENTS.md) maps the code and says how to add a packet or a window.

## Pull requests

- Keep one change to a pull request, and say what changed, why, and how you checked it.
- `cargo fmt` (160 columns) and `clippy -D warnings` stay clean.
- `ponytail:` comments mark deliberate shortcuts, with what would replace them.
- Your contribution is taken under the same terms as the project: MIT OR Apache-2.0.
