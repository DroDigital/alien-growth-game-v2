# Status

_Last updated: M0 Foundation (2026-11-02)._

## Milestone: M0 Foundation (done, pending your review)

### Works

- **Workspace**: `game_core`, `game`, `launcher`, `tools`. Every dependency is pinned exactly.
  Bevy is 0.20.1 and Rust is 1.99.1.
- **M1 build settings**: `jobs = 4`; dev opt-level 1 for our crates and 3 for dependencies
  (no dependency debug info); `dynamic_linking` only through the `dev` feature. The cargo
  aliases `cargo dev`, `cargo t` and `cargo t-tools` share one Bevy build.
- **Game skeleton**: flow states (`AppState` and the `RunState` sub-state), one plugin per
  domain (most are stubs), a leafwing input map, a Fluent-localized main menu and pause menu,
  quality presets from `data/presets.ron` applied live to the camera, the render-scale camera
  rig, and a crash hook plus running marker.
- **Testbed scene** (Tier 1 mood in grey-box): crater seafloor, instanced rocks, a meteor with
  glowing seams, the organism with glowing veins, bioluminescent motes, GPU marine snow
  (Hanabi), distance fog, an underwater gradient env map with matching skybox, and volumetric
  fog on Medium and up.
- **Screenshot mode**: `game --screenshot <scene> <preset> <out.png>` renders headless and
  offscreen through the same render-scale path as the window. It waits until every pipeline
  is compiled. Verified on lavapipe (software Vulkan): Low captures with 77 pipelines and
  Medium with 83, 0 failed.
- **Benchmark mode**: `game --benchmark [secs]` runs a testbed flythrough in a 1080p window
  with vsync off. It writes a JSON report with average fps, 1% low, max frame time, spikes
  over 50 ms and process memory, and recommends a preset. Verified end to end under Xvfb and
  lavapipe: 646 MiB resident, fps meaningless on a CPU rasterizer.
- **Render resolution** follows the preset rather than the display: the output is capped to
  the preset's reference height (1080 for Low and Medium), then scaled. Low on a 2560×1600
  Retina panel therefore renders 1296×810 and upscales.
- **Warm-up mode**: `game --warmup` reloads the testbed under each preset (Low → Ultra) until
  pipelines settle: 85 pipelines, 0 failed. This also exercises live preset switching and
  scene reloads. The persistent pipeline cache comes in M2.
- **Asset pipeline**: `assets/sources.json` → `tools/fetch_assets.py` (Poly Haven, ambientCG,
  Freesound with a CC0 filter, direct URLs; zip extraction, caching, `--locked`, `--ultra`)
  → `agtool texconv` (KTX2 with BC7/BC5/BC4/BC6H, mips and zstd; equirect HDR → cubemap)
  → `assets/manifest.json` + `CREDITS.md`. The game falls back to procedural content per
  asset.
- **Launcher skeleton**: Play button, preset, backend, custom args, after-launch behavior,
  "launch directly", Safe Mode after an abnormal exit, a basic system scan (OS, CPU, RAM,
  disk), and localized strings. `launcher --headless-launch ARGS` starts the game and returns
  its exit code.
- **Build and CI**: `tools/build.sh [mac|windows|linux] [debug|release]`, plus GitHub Actions:
  lint (fmt, clippy, data, credits), tests with screenshots on lavapipe, a macOS arm64 build
  and a Windows x64 build, all uploading artifacts.

### Stubbed / not yet done

- Gameplay plugins (Creature, Ecosystem, Strata, Mutation, Threat, AI, Audio, Save, Mod,
  Online) are empty shells. Audio only initializes Kira.
- Asset hashes in `assets/manifest.json` are `null` until the first real fetch. The container
  used for M0 could not reach Poly Haven or ambientCG (network policy), so the download
  path has not run against the real hosts yet. Run `python3 tools/fetch_assets.py` on the
  Mac once and commit the updated manifest and CREDITS.
- In-game settings menu (launcher settings editor is M2), rebinding UI, colorblind modes.
- SSR, irradiance volumes, DoF focus tracking, custom post-process nodes (caustics, absorption):
  M2 visual pass.
- Launcher: GPU/driver/display scan, benchmark/warm-up tabs, integrity, updates, mods, crash
  upload, diagnostics zip, controller navigation (keyboard focus works; gamepad via gilrs is
  M2): M2/M6.

### Bugs found and fixed while verifying M0

- `WgpuSettings.backends = None` means "no renderer" in Bevy 0.20, so "auto" now keeps the
  defaults.
- Re-inserting `Camera` on scene reload zeroed its viewport for a frame, and GPU clustering hit
  a wgpu validation error. Scenes now set `ClearColor` instead.
- `SystemInformationDiagnosticsPlugin` was registered twice (benchmark + debug overlay), and
  Bevy disables it under dynamic linking anyway. Memory now comes from `sysinfo` directly.
- The glow shader wrote `emissive.a = 1` (full camera-exposure weighting), which crushed the
  emission to near zero.

### Performance

- Not measured on the M1 yet; that needs your Mac (see "Next steps").
- In the container, lavapipe (CPU rendering) captures a 1920×1080 Low frame in about 20 s,
  including shader compilation. That says nothing about GPU performance.

### Next steps (M1 Vertical slice)

1. On the Mac, check the dev loop: `tools/build.sh mac debug`, then `cargo dev`. The window,
   menu, Esc pause and F3 perf overlay should all work.
2. On the Mac, get official numbers from a release build: `tools/build.sh mac release`, then
   `dist/mac-release/game --benchmark 30 --preset low`. Report the `BENCHMARK` line
   (avg fps, 1% low, max frame, memory).
3. On the Mac: `python3 tools/fetch_assets.py`, then commit `assets/manifest.json` and `CREDITS.md`.
4. M1 work: Devourer strain, metabolism (`data/balance/*.ron`), marching-cubes body, Tier 1–2
   grey-box, consumption feel, death, restart.

## How to resume

1. Read `docs/ARCHITECTURE.md`.
2. `cargo dev` runs the game; `cargo t` runs the tests.
3. `cargo dev -- --screenshot testbed low screenshots/x.png` checks visuals.
