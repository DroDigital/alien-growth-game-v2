# Backlog

Ideas and follow-ups outside the current milestone. Move items into a milestone when it starts.

## Rendering

- Bevy 0.20 ships built-in DLSS (`dlss` feature, Vulkan, needs the NVIDIA SDK at build time).
  Evaluate for High/Ultra once a Windows test machine is available; FSR has no maintained
  integration yet.
- `bevy_solari` (experimental ray-traced lighting) exists in 0.20. Candidate for the optional
  Ultra RT toggle; gate on hardware ray-tracing support.
- Enable the `bluenoise_texture` Bevy feature for better SSR/env-filter noise (costs a texture
  download at build time).
- Render-scale upscale is plain bilinear. Add an RCAS-style sharpening pass after upscale for
  render_scale < 1.
- Skip the offscreen copy when render_scale == 1.0 (render the world camera straight to the
  window) if profiling shows the extra pass matters.
- World-space vein noise in `bioluminescence.wesl` swims on moving meshes. Switch to an
  object-space or UV pattern for the creature (M1 creature shader).

- Creature bodies read as near-spheres at later growth stages: the `r²/d²` kernel blends
  lobes too smoothly. Try a compact Wyvill kernel `(1 - d²/R²)³` for lumpier silhouettes
  (also shrinks the meshing box).
- Tier 2 sand washes out near the camera on Low. Needs a reef visual pass (caustics, darker
  albedo, absorption with depth): M2.

## Gameplay / balance (seen in M1 bot runs)

- Bots arriving in Tier 2 often die within the first minute to sea-cucumber and sea-turtle
  struggles or reef sharks. Watch for this in playtests; it may need a short grace period or a
  gentler arrival zone.
- The bot policy is a stand-in for human play. Add a "replay my run" bot that mimics a
  recorded human input distribution once there is playtest data.
- Death-screen copy uses species names mid-sentence ("Eaten by a Lanternfish fry"). Add Fluent
  grammatical cases or lowercase variants per language.

## Pipeline / tooling

- Validate KTX2 output with Khronos `ktx validate` in CI (download KTX-Software in the
  workflow).
- Persist the wgpu pipeline cache (`wgpu::PipelineCache` where supported: Vulkan) in
  `--warmup` (M2 launcher v1).
- `cargo-hakari`-style workspace-hack crate if feature unification between workspace-wide and
  per-crate builds keeps costing disk space.
- Kenney/Quaternius packs: no stable download API. Use the `direct` provider with
  mirrored zip URLs once chosen.

## Launcher

- Gamepad navigation via `gilrs` (M2), with localized controller glyphs.
