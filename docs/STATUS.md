# Status

_Last updated: M1 Vertical slice (2026-11-03)._

## Milestone: M1 Vertical slice (done, pending your playtest)

Scope: Devourer strain; Tiers 1–2 in grey-box; metabolism, growth and consumption feel;
death and restart; Ascension mode only.

### Works

- **Run simulation in `game_core`** (64 Hz `FixedUpdate`, no rendering):
  - Metabolism: Kleiber upkeep, digestion queue, growth, Gorged state, starvation.
  - Size gate: edible, struggle or dangerous.
  - Struggles (mash Consume), hunters with stamina, bites and knockback.
  - Tier completion, the run log, and death causes.
  - Every number comes from `data/balance/*.ron`, with a built-in fallback. `agtool
    validate-data` checks the files.
- **Tier 1, The Impact Site**: a crater seafloor with six species (diatom → lanternfish fry).
  **Tier 2, The Shallows**: an alien coral reef with seven species (reef shrimp → reef shark).
  Both are grey-box worlds built from the same seeded terrain the simulation collides with.
- **Devourer**: camera-relative swimming (WASD/left stick), ascend and descend (Space/Ctrl,
  bumpers), dash (Shift/A), and Consume (left mouse/RT), mashed during struggles.
- **Body**: metaballs meshed with marching cubes off the main thread. It is re-meshed at each
  growth stage, gets more lobes as it grows, and has a glowing, wobbling skin.
- **Feel**:
  - Hit-stop on big meals, bites and struggle results.
  - Trauma camera shake, gamepad rumble and a swallow animation.
  - The camera pulls back as you grow.
  - Synthesized SFX (gulp, crunch, bite, grapple, tier-up swell, death), pitched lower as you
    get bigger.
  - Shake, rumble and SFX captions follow the accessibility settings.
- **HUD**: biomass, tier progress, energy, integrity, stomach (with the Gorged threshold) and
  the struggle meter. Status tags ([HUNGRY], [GORGED], …) carry text, not just color. Prey
  size classes use a colorblind-safe palette plus shape markers (ring = struggle, cone =
  dangerous).
- **Tier-up**: a stratum title card, plus a 3.6 s zoom-out cinematic with the world swapped
  behind a fade. Confirm or Back skips it.
- **Death and restart**:
  - The death screen shows the cause ("Eaten by a Lanternfish fry."), time, peak biomass,
    meals, the tier reached, the last 10 timeline events, and a heatmap of where you roamed.
  - Retry starts a fresh run immediately; Main menu returns to the title.
  - Completing Tier 2 shows a Victory screen.
- **Saves**: an autosave at the start of each tier (versioned RON). The main menu offers
  "Continue (Tier N)". Death, victory and Abandon delete the save (permadeath).
- **Balance bots**: `cargo bots --runs N`. These are headless, deterministic per seed and run
  in parallel, with a play style drawn from the seed.
- **Tooling**:
  - Run screenshots: `--screenshot run-tier1|run-tier2|death <preset> <out>`, bot-driven at
    an accelerated clock.
  - `--benchmark` now measures a bot playing Tier 2. It logs memory and entities every minute
    for soaks. The M0 flythrough is `--benchmark-scene testbed`.
  - `--skip-menu` and `--seed N` for dev runs.
- **Debug**: the F2 live balance panel edits the running simulation's numbers; Reload re-reads
  `data/balance`.
- **CI**: adds a `balance-bots` job (400 runs, fails above 40%) and bot-driven run screenshots
  on lavapipe.

### Acceptance checks

| Check | Result |
|---|---|
| Clean clone builds | `cargo build --workspace` (CI) |
| CI Windows build | `build-windows` job (unchanged from M0) |
| Tests | 57 pass in `game_core` + `game` (`cargo t`); launcher 9, tools 1 |
| Clippy | clean with `-D warnings`, with and without `game/dev` |
| Bots: no death cause > 40% at Hostility 0 | **26.7% max** (1000 runs, seeds 1–1000; see below) |
| Low preset meets the M1 target (60 fps at 1080p render-scaled, < 3.5 GB) | **needs your Mac**: see "Performance" |
| Every asset in the manifest | M1 adds no files: all visuals are procedural and SFX are synthesized. `agtool verify-assets` passes |
| No hardcoded balance values | all tunables live in `data/balance/*.ron`. Presentation constants (camera, UI, SFX) are not balance |
| 30-minute soak | SOAK_RESULT |

**Balance bots** (`cargo bots --runs 1000 --seed 1`, 532 s on 4 cores):

| | |
|---|---|
| Completed both tiers | 18.8% (mean 630 s) |
| Reached Tier 2 | 33.1% |
| Mean run | 294 s |
| Mean peak biomass | 941 |

Death causes, as a share of deaths:

| Cause | Share |
|---|---|
| Lanternfish fry (predation) | 26.7% |
| Starvation | 20.4% |
| Comb jelly (struggle) | 15.5% |
| Reef shark (predation) | 9.0% |
| Copepod (struggle) | 9.0% |
| Arrow worm (predation) | 5.4% |
| Sea turtle (struggle) | 5.4% |
| Brine shrimp (struggle) | 5.2% |
| Everything else | under 1% each |

### Stubbed / not yet done

- Hostility levels, other strains, mutations, Threat, Tier 3+, the Genetic Memory payout
  (the death screen shows 0) and other game modes: later milestones.
- Visuals are grey-box: there are no fetched textures yet (`tools/fetch_assets.py` still has
  to run on your Mac), and Tier 2 sand washes out near the camera on Low (see Backlog).
- Audio: SFX only; the adaptive score and mixing buses are M2. No audio device was available
  in the container, so I have not heard the sounds.
- Rebinding UI, in-game settings menu and colorblind mode selection (the palette is already
  colorblind-safe by default) are M2. The in-game console is not started.
- The tier-up cinematic and Retry/Continue flows are only exercised by code paths and unit
  tests, not by screenshots. Please check them in the playtest.

### Bugs found and fixed while verifying M1

- The bots starved with 0 meals: they fled any hunter in range and never fed. They now keep
  feeding while steering away, and bolt only when a threat is close. The batch went from
  ~48% starvation to 20%.
- The creature mesh had holes at later growth stages: the marching-cubes box was too tight
  for the summed field. The box now pads by √(Σr²), and a watertight test covers stages
  0–20 at every preset resolution.
- Pipelines waiting on a shader load were counted as failed, so the benchmark aborted on a
  false positive. They now count as pending.
- The struggle made the player immune to bites. Bites now land during struggles.
- The death screen overflowed at 540p, and HUD labels were unreadable over bright sand.
  Fixed with a compact layout and a HUD backdrop.

### Performance

- **Not measured on the M1 yet; that needs your Mac.**
- In the container (llvmpipe, a CPU rasterizer, so the fps means nothing): the Tier 2 bot
  benchmark at Low 1080p (render 1440×810) runs about 2.3 fps.
  - Resident memory is 848 MiB, including the software renderer.
  - There are about 1,400 entities.

### Next steps

1. **Playtest on the Mac** (debug build with hot reload):
   ```
   cargo dev
   ```
   Things I can't judge without your eyes and hands:
   - Does eating feel good (hit-stop, shake, gulp)?
   - Is mashing in struggles readable?
   - Is the camera distance right at small and large sizes?
   - Do the tier-up cinematic, death screen, Retry and Continue all behave?

   F2 tunes balance live; F3 shows performance.
2. **M1 performance numbers** (release):
   ```
   tools/build.sh mac release
   dist/mac-release/game --benchmark 60 --preset low
   ```
   Paste the `BENCHMARK` line. The target is 60 fps avg at Low with memory under 3.5 GB.
3. Optionally, a 30-minute soak on the Mac:
   `dist/mac-release/game --benchmark 1800 --preset low`.
4. M2 (after your feedback): the visual pass, adaptive audio and launcher v1, per the plan.

## How to resume

1. Read `docs/ARCHITECTURE.md`.
2. `cargo dev` runs the game; `cargo t` runs the tests; `cargo bots --runs 200` checks balance.
3. Check visuals with
   `cargo dev --screenshot run-tier1 low screenshots/x.png --resolution 1280x720` (also `run-tier2`,
   `death`, `testbed`, `reef`, `menu`).

## M0 Foundation (done, merged in PR #1)

Workspace, pinned dependencies and M1 build settings; game skeleton with flow states and
domain plugins; screenshot, benchmark and warm-up modes; asset pipeline (fetch_assets.py →
KTX2 → manifest and credits, with procedural fallbacks); launcher skeleton; build script and
CI for macOS, Windows and Linux. Pending from M0: run `python3 tools/fetch_assets.py` on the
Mac and commit the manifest and CREDITS.
