# Modding

> Status: planned for M6. This page describes the intended format so data added in M1–M5
> stays mod-friendly. Nothing here is loaded yet.

## What mods can change

- **Data mods**: RON/JSON files that override or extend `data/` (balance, mutations, species,
  strata event tables, presets). They are merged by id, so a mod can replace one mutation
  without copying the file.
- **Asset mods**: textures, models, sounds and `.ftl` translations placed in the same relative
  paths as `assets/`.

## Layout

```
<user data dir>/mods/<mod-id>/
  mod.ron          # id, name, version, game_version range, load_after, description
  data/...         # same relative paths as the game's data/
  assets/...       # same relative paths as the game's assets/
```

## Rules

- Load order is set in the launcher's Mods tab. Later mods win. The launcher reports
  conflicts: two mods touching the same id.
- `--safe-mode` and `--no-mods` disable every mod.
- Mods never execute code; they are data and assets only.
- Achievements and leaderboards are disabled while gameplay-affecting data mods are active
  (the same rule as Mutator Lab).
