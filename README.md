# Alien Growth (working title)

A desktop 3D roguelike growth game in Rust and Bevy. A microscopic alien organism crashes
into Earth's ocean and eats its way from microbe to universe.

Status: **M0 Foundation**. See [docs/STATUS.md](docs/STATUS.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Quick start

```bash
cargo dev                      # run the game (dev build: dynamic linking, hot reload, F1/F3 debug UI)
                               # game flags follow directly: `cargo dev --preset low` (the alias already ends in `--`)
cargo launcher                 # run the launcher
cargo t                        # game_core + game tests (reuses the dev build)
cargo t-tools                  # launcher + tools tests
```

Release and packaged builds:

```bash
tools/build.sh mac debug       # or: mac|windows|linux  debug|release
```

Visual check without a window:

```bash
cargo dev --screenshot testbed low screenshots/testbed_low.png
```

Performance (windowed, vsync off, writes JSON):

```bash
cargo dev --benchmark 30 --preset low
```

Optional third-party assets (CC0). The game runs without them using procedural fallbacks:

```bash
python3 tools/fetch_assets.py
```

Requirements: Rust (the toolchain is pinned in `rust-toolchain.toml`; rustup installs it).
On Linux you also need `libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev`.
