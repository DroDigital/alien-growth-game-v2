# Architecture

Read this file and [STATUS.md](STATUS.md) to resume work without rereading the codebase.

## Workspace

```
crates/
  game_core/   pure rules, balance data, the fixed-step run simulation, balance bots,
               saves, config, localization, manifest, perf stats (no rendering)
  game/        the Bevy app: plugins per domain, rendering, UI, tooling modes
  launcher/    eframe/egui launcher (no Bevy)
  tools/       agtool: texconv (KTX2), validate-data, verify-assets, credits
data/          RON data: presets.ron, balance/{metabolism,strains,strata}.ron
assets/        locale/*.ftl, sources.json, manifest.json, fetched assets (gitignored)
shaders/       WESL/WGSL, loaded through the `shaders://` asset source
tools/         build.sh, fetch_assets.py
docs/          this file, STATUS, Backlog, Release, Modding
```

`game_core` has a `bevy` feature (on by default) for ECS integration (states, headless
plugins). The launcher and tools depend on it with `default-features = false`, so they never
compile Bevy.

## Dependency policy

- Every crate is pinned exactly (`=x.y.z`) in the root `Cargo.toml` `[workspace.dependencies]`.
- Bevy is **0.20.1**. All plugins have releases for it: avian3d 0.8.0, bevy_hanabi 0.20.0,
  bevy_kira_audio 0.27.1, leafwing-input-manager 0.22.0, bevy_egui 0.43.1,
  bevy-inspector-egui 0.38.0, bevy_common_assets 0.18.0. **big_space has only a release
  candidate (0.14.0-rc.0, targets ^0.20.0-rc.2)**; switch to 0.14.0 when it ships.
- Toolchain is pinned in `rust-toolchain.toml` (1.99.1; Bevy 0.20 needs ≥ 1.97.1).
- Upgrade Bevy only between milestones, after every plugin above has a compatible release.
- Reserved pins for later milestones are listed (commented) in `Cargo.toml`.

## Build settings (8 GB M1)

- `.cargo/config.toml`: `jobs = 4`, `BEVY_ASSET_ROOT` = workspace root for `cargo run`.
- `[profile.dev]` opt-level 1 + line tables for our crates; `[profile.dev.package."*"]`
  opt-level 3, no debug info for dependencies (fast at runtime, small `target/`).
- `game` feature `dev` = `bevy/dynamic_linking` + `bevy/file_watcher` + `debug-ui` (egui).
  Only debug builds enable it (`cargo dev`, `tools/build.sh … debug`).
- Feature unification: building the whole workspace at once yields a *different* Bevy build
  than `-p game` (eframe/winit and image features unify). Use the aliases so day-to-day work
  shares one Bevy build:
  - `cargo dev [-- ARGS]`: run the game (dynamic linking)
  - `cargo t`: tests for game_core + game, sharing the `cargo dev` artifacts
  - `cargo t-tools`: launcher + tools tests (no Bevy)
  - `cargo agtool <cmd>`: build tools, release
- Disk: `cargo sweep --time 14` or `cargo clean` regularly; budget is 20 GB total.

## Runtime layout (game crate)

`main.rs` parses the CLI (`cli.rs`), loads `settings.ron` (with `--preset` / `--safe-mode`
overrides), registers the `shaders://` asset source and builds the app:

| Plugin | File | Role |
|---|---|---|
| CorePlugin | `boot.rs` | flow states, `Balance` (from `data/balance`, built-in fallback), `game_core::sim::SimPlugin`, presets → `ActiveRenderSettings`, `Locale`, asset availability, Boot → Menu/Run/Tooling |
| InputPlugin | `input.rs` | leafwing `GameAction` map on a `PlayerInput` entity |
| GraphicsPlugin | `graphics/` | camera rig, render scale, per-preset effects, materials (`GlowMaterial`, `CreatureMaterial`), environment, follow camera, pipeline-status bridge, Hanabi |
| ScenesPlugin | `scenes/` | static scenes (`testbed`, `menu`, `reef`), `LoadScene`, `OrbitShot`; `worlds.rs` builds crater/reef worlds for scenes and runs alike |
| UiPlugin | `ui/` | theme, main menu (Continue when a save exists), pause, HUD (`hud.rs`), death/victory screen (`death.rs`) |
| ToolingPlugin | `tooling/` | `--screenshot` (static and bot-driven run scenes), `--benchmark`, `--warmup` |
| CreaturePlugin | `creature/` | player body (metaballs → marching cubes off-thread), sim→Transform interpolation, device input → `PlayerIntent`, cursor grab, feel (hit-stop, shake, rumble) |
| EcosystemPlugin | `ecosystem.rs` | prey/predator visuals per species, size-class colors + shape markers, swallow animation |
| StrataPlugin | `strata.rs` | starts runs (`PendingRun`), builds each tier's world on `TierEntered`, title card, tier-up cinematic |
| AudioPlugin | `audio.rs` | synthesized SFX pitched by size, SFX captions |
| SavePlugin | `save.rs` | run autosave per tier, deleted on death/victory/abandon |
| Mutation, Threat, Ai, Mod, Online | `*.rs` | domain stubs; filled in later milestones |
| DebugPlugin | `debug.rs` | `debug-ui` feature only: F1 inspector, F2 live balance panel, F3 perf overlay |

Rules for new code:

- Game rules live in `game_core` and are unit-tested without rendering.
- Cross-plugin communication goes through Bevy messages (`#[derive(Message)]`,
  `MessageWriter/Reader`) or observers, never direct calls into another plugin's systems.
- Simulation runs in `FixedUpdate`; presentation in `Update`.
- No hardcoded balance values: they go in `data/balance/*.ron` (from M1).
- Every user-facing string comes from `assets/locale/<lang>/*.ftl`.

### Flow states (`game_core::states`)

`AppState`: `Boot → MainMenu | InRun | Tooling`. `RunState` (sub-state of `InRun`): `Playing`,
`Paused`, `TierTransition`, `Dead`, `Victory`. Use `DespawnOnExit(state)` for state-scoped
entities. Run screenshots and the run benchmark go `Tooling → InRun`.

## Simulation (`game_core`)

Everything that decides what happens in a run lives here and runs without rendering, so the
game, the tests and the balance bots execute the same code.

- **Balance** (`balance.rs`): `Balance { core, strains, strata }`, a Bevy resource loaded from
  `data/balance/*.ron` and validated. The same files are compiled in (`Balance::builtin`) as
  the fallback. Every tunable number is here: no balance literals in systems.
- **Rules** (`rules.rs`, pure functions + `Vitals`): radius ∝ biomass^(1/3); speed ∝ r^0.8;
  size gate (prey/player radius ≤ 0.8 edible, ≤ 1.2 struggle, above dangerous); Kleiber
  upkeep `k_tier × strain × B^0.75` in absolute energy, shown as % of capacity
  (`capacity_per_biomass × B`); a digestion queue converts stomach mass into energy and
  growth; Gorged above 150% fill (slower, takes more damage); starvation drains integrity;
  `Struggle` (mash Consume against size-scaled resistance); `DeathCause`.
- **Terrain** (`terrain.rs`): deterministic height field per biome and seed (crater with rim;
  reef with bommies), shared by the sim (floor collision) and the world mesh.
- **Run sim** (`sim/`): `SimPlugin` runs in `FixedUpdate` at 64 Hz in chained `SimSet`s
  (Intent → Ai → Movement → Interactions → Metabolism → Progress), only while
  `RunState::Playing`. Components: `Pos`/`PrevPos`/`Vel`/`Radius` (presentation interpolates
  with `overstep_fraction`), `Player` + `PlayerVitals`, `Agent` (species, behavior, mode,
  stamina). `PlayerIntent` is the only input (written by the device-input system or a bot).
  Messages out: `Consumed`, `StruggleStarted/Ended`, `PlayerBitten`, `TierCompleted`,
  `TierEntered`, `PlayerDied`. `RunLog` keeps the timeline and 2 s position samples for the
  death screen. `start_run(world, seed, strain, checkpoint)` and `advance_tier(world)` are
  the entry points.
- **Saves** (`save.rs`): `RunSave { version, game_version, checkpoint }` in RON, atomic write,
  versioned with `migrate`.
- **Balance bots** (`bots.rs`, `bin/balance_bots.rs`): `headless_app` = `MinimalPlugins` +
  states + `SimPlugin` + `BotPlugin`, single-threaded schedules and a manual fixed clock, so a
  seed always produces the same run. `BotProfile` (mash rate, caution, greed, sloppiness)
  comes from the seed, so a batch covers different play styles. The policy flees nearby
  hunters, feeds while steering away from distant ones, picks the best mass-per-distance
  target, and avoids risky struggles after losing one. `cargo bots --runs N [--seed S]
  [--json out.json] [--assert-max-cause 0.4] [--data dir]` runs batches in parallel (rayon)
  and prints death causes, tier reach and completion time. CI fails if any death cause
  exceeds 40% of deaths.

## Presentation (game crate)

- **Creature body** (`creature/mesher.rs`): metaballs (`r²/d²` field, iso 1) polygonized
  with marching cubes (Bourke's tables, `mc_tables.rs`) on the `AsyncComputeTaskPool`, with
  vertices shared per grid edge (watertight, smooth normals from the field gradient). The grid
  box pads every centre by √(Σr²), which provably contains the surface. A new mesh is built
  when the growth stage (`floor(2·log2 B)`) changes; the resolution follows the preset.
- **CreatureMaterial**: the glow material plus a vertex wobble in `bioluminescence.wesl`.
  It is alpha-blended, so it skips the depth prepass; that avoids a depth mismatch with the
  displaced vertices.
- **Follow camera** (`graphics/follow.rs`): orbit around the player (mouse/right stick), with
  distance and FOV scaling with radius, floor clearance, trauma-based shake, and `zoom_out`
  driven by the tier-up cinematic.
- **Feel** (`creature/feel.rs`): hit-stop pauses `Time<Virtual>` for a few real-time frames;
  shake and rumble scale with the event. All respect the accessibility settings.
- **Ecosystem visuals**: shared meshes per shape, materials per species × size class. Classes
  use a colorblind-safe palette plus shape markers (torus = risky, cone = dangerous), never
  color alone.

### Rendering

- **Camera rig** (`graphics/mod.rs`): `WorldCamera` (Camera3d, HDR, AgX tonemapping) renders
  into an offscreen image at `RenderSettings::render_resolution(output)`: the output is capped
  to the preset's reference height (1080 lines for Low and Medium), keeping aspect ratio, then
  multiplied by `render_scale`. GPU cost then follows the preset rather than the display.
  `PresentCamera` (Camera2d + default UI camera) shows it full-screen (bilinear upscale) and draws UI at native resolution. Headless
  modes point `PresentCamera` at an offscreen output image instead of the window.
- **Presets** (`data/presets.ron`, `game_core::config`): Low/Medium/High/Ultra
  `RenderSettings`; `QualityOverrides` in settings.ron apply on top. `apply_render_settings`
  inserts/removes TAA/SMAA/FXAA, Bloom, SSAO, VolumetricFog, DoF, MotionBlur and sets the shadow
  map size whenever `ActiveRenderSettings` changes. SSR (needs the deferred path) and irradiance
  volumes are M2.
- **Shaders**: Bevy 0.20 ships its PBR library as WESL, so material shaders that import Bevy
  modules are `.wesl` (WGSL syntax plus `import`). Standalone shaders (compute boids,
  post-process) can be plain `.wgsl`. Path: `shaders://name.wesl`.
- **GlowMaterial** = `ExtendedMaterial<StandardMaterial, Bioluminescence>`
  (`shaders/bioluminescence.wesl`): body pulse + fresnel rim + ridged-noise veins. Keep
  `emissive.a` (exposure weight) at 0 so emission is not scaled by camera exposure.
- **Environment**: procedural underwater gradient cubemap (`graphics/env.rs`), GPU-filtered by
  `GeneratedEnvironmentMapLight` and reused as the `Skybox`. Distance fog color matches the
  palette horizon.
- **Pipeline status** (`graphics/pipelines.rs`): a render-world system counts
  ready/pending/failed pipelines into an `Arc` shared with the main world. Tooling waits for
  10 stable frames before capturing or measuring.

### Tooling modes

- `--screenshot <scene> <preset> <out.png> [--resolution WxH] [--settle-frames N]`: headless
  (no window, no winit, `ScheduleRunnerPlugin`), offscreen output, exits 0 on success, 1 if
  pipelines failed, 2 on bad args, 3 on watchdog timeout (600 s), 5 if a run scene never
  reached its moment. Static scenes: `testbed`, `menu`, `reef`. Run scenes: `run-tier1`,
  `run-tier2`, `death`. These start a real run with a fixed seed, driven by the balance bot
  (`AutoPilot` disables device input). The clock is manual at 0.25 s of sim per frame, and
  the world freezes (virtual speed 0) once the moment arrives, before settling and capturing.
- `--benchmark [secs] [--benchmark-scene run|testbed] [--benchmark-out file]`: windowed,
  vsync off. The default `run` scene has the bot play Tier 2, the heaviest world, in real
  time, restarting on death. `testbed` is the M0 flythrough. Writes
  `game_core::perf::BenchmarkReport` JSON (avg fps, 1% low, max frame, spikes over 50 ms,
  memory), prints a `BENCHMARK …` line, and logs memory and entity count every minute (soak
  runs: up to 3600 s).
- Dev flags: `--skip-menu` starts a run directly; `--seed N` fixes the run seed.
- `--warmup`: loads each warm-up scene under each preset until pipelines settle. Persisting a
  wgpu pipeline cache is M2.

## Paths (`game_core::paths`)

- Content root: `BEVY_ASSET_ROOT` (set by cargo), else the exe dir, else a macOS bundle's
  `Contents/Resources`. Contains `assets/`, `data/`, `shaders/`.
- User dirs: `AGG_USER_DIR` override, else OS dirs (`directories`, "dev.drodigital.alien-growth"):
  `config/settings.ron`, `saves/`, `logs/` (crash reports), `cache/` (benchmark.json,
  `game.running` marker, pipeline cache).

## Launcher ↔ game contract

- Flags are constants in `game_core::launch`. The launcher passes `--preset`, `--backend`,
  `--safe-mode` and the user's custom args (quote-aware split).
- Both read/write the same `Settings` RON (`game_core::config::Settings`, versioned, migrates,
  atomic save).
- Crash detection: the game writes `cache/game.running` at start and removes it on a clean exit.
  A leftover marker means an abnormal exit, and the launcher offers Safe Mode.
- `launcher --headless-launch [ARGS]`: start the game without UI and return its exit code
  (smoke tests/CI). The launcher adds its own directory to the dylib search path so
  dynamically linked dev builds start.

## Asset pipeline

1. `assets/sources.json` (hand-edited): provider (`polyhaven`, `ambientcg`, `freesound`,
   `direct`), asset id, map, resolution, conversion kind, output path, procedural fallback name,
   `ultra_pack` flag.
2. `tools/fetch_assets.py` resolves URLs and authors through provider APIs, downloads into
   `assets/.cache/`, extracts zip members, converts with `agtool texconv`, and writes
   `assets/manifest.json` (URLs, author, license, `source_sha256`, `sha256`) and `CREDITS.md`.
   `--locked` uses only manifest URLs and fails on hash changes (reproducible CI);
   `--ultra` adds the 4K pack.
3. `agtool texconv`: PNG/JPG/HDR → KTX2 with full mips + Zstandard supercompression; BC7 sRGB
   (color), BC7 linear (roughness packed into G, Bevy's metallic-roughness layout), BC5 (normals,
   which Bevy detects as two-channel), BC4 (masks), BC6H (HDR, equirect → cubemap). BCn is
   native on Windows GPUs and Apple Silicon.
4. The game reads the manifest at boot (`AssetAvailability`). Missing or corrupt files fall back
   to procedural content. The game never downloads.
5. CI: `agtool verify-assets` (license allowlist, https, unique ids, hashes) and
   `agtool credits --check`.

## CI (`.github/workflows/ci.yml`)

- **lint**: fmt, clippy `-D warnings`, data/manifest/credits validation.
- **test**: workspace tests, Low and Medium testbed screenshots, plus bot-driven `run-tier1`
  and `death` screenshots on lavapipe (software Vulkan), and the launcher-launches-game smoke
  test. Screenshots are uploaded as artifacts.
- **balance-bots**: `cargo bots --runs 400 --assert-max-cause 0.4`, with the JSON report
  uploaded.
- **build-macos** (macos-14, arm64) and **build-windows** (windows-2022): tests for core and
  tools, `tools/build.sh <os> release`, artifacts uploaded.
