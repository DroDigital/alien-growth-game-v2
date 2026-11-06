# Alien Growth: End Vision

_Last updated: 2026-11-06. Companion documents: [STATUS.md](STATUS.md) (where we are), [ARCHITECTURE.md](ARCHITECTURE.md) (how it is built), [Backlog.md](Backlog.md) (ideas outside the current milestone), [Modding.md](Modding.md), [Release.md](Release.md)._

This document describes the finished game. It is expanded from the original project specification, which is the authority. Where this document adds detail the specification does not give, it says so. Three labels appear throughout:

- **[Spec]**: stated in the original specification.
- **[Derived]**: follows from the specification or from an engineering constraint. My inference, to be confirmed.
- **[Open]**: the specification does not decide this. A decision is needed before the milestone that depends on it. The full list is in section 29.

---

## Contents

0. Summary
1. Identity and constraints
2. Experience pillars
3. The player's experience
4. The core loop
5. Moment-to-moment play
6. Growth and scale
7. Survival systems
8. Consequence systems: ecosystem, Threat, Instability
9. Mutations
10. Strains
11. The world: strata, routes, generation
12. Creatures and prey
13. Progression and replayability
14. Game modes
15. Narrative
16. Presentation and art direction
17. Audio
18. UI, UX and accessibility
19. Localization
20. Launcher
21. Platforms and performance targets
22. Technical architecture
23. Content and asset pipeline
24. Quality, testing and verification
25. Roadmap
26. Acceptance criteria
27. Scope reality check and the "AAA" bar
28. Where the current build departs from this vision
29. Open decisions
30. Glossary

---

## 0. Summary

Alien Growth is a desktop 3D roguelike about a microscopic alien organism. It crashes into Earth's ocean inside a meteor and grows by eating, climbing from microbe to reef, ocean, coast, continent, civilization, planet, solar system and finally the universe. Each run is a single life with permanent death. Between runs, Genetic Memory, a returning rival and a branching world make every attempt different.

The finished game contains ten strata (world regions), five playable strains, eight game modes (one of them stretch), five endings, about sixty achievements, a launcher with system scanning, benchmarks, shader warm-up, integrity checks, signed updates and a mod manager, Steam integration, and localization for ten or more languages. It is written entirely in Rust, WGSL shaders and data files, on Bevy, with no editor. Its look comes from light, atmosphere and motion rather than polygon counts. [Spec]

---

## 1. Identity and constraints

| Item | Definition | Source |
|---|---|---|
| Genre | 3D roguelike growth and survival, with a simulated food web | [Spec] |
| Working title | The specification uses a placeholder. The code currently calls the game "Alien Growth" (`GAME_TITLE`). | [Open] Q1 |
| Players | Single player for launch. Versus and co-op is stretch (2–8 organisms in one stratum). | [Spec] |
| Platforms | Windows 10/11 64-bit (primary). macOS on Apple Silicon. Linux is a stretch goal. | [Spec] |
| Graphics API | Direct3D 12 or Vulkan on Windows. Metal on macOS. Everything goes through wgpu. | [Spec] |
| Engine | Bevy, pinned exactly. Upgraded only between milestones, after every plugin supports the new version. | [Spec] |
| Language | Rust for code, WGSL for shaders (WESL where Bevy imports are needed), RON and JSON for data. No editor. | [Spec] |
| Dev machine | Apple M1, 8 GB unified memory, about 40 GB free disk. The game must run on it at the Low preset. | [Spec] |
| Disk budget | Toolchain, `target/` and project under 20 GB. Base asset set under 5 GB. | [Spec] |
| Business model, pricing, store plan | Not in the specification. | [Open] Q2 |
| Steam | Steam integration is in scope. It sits behind a trait so non-Steam builds work. | [Spec] |

**What the game is not** (derived from the specification's pillars and art direction):

- Not a photorealistic game. A cohesive art direction matters more than photorealism. [Spec]
- Not a biology simulator. The food web is simplified and Lotka–Volterra-style. [Spec]
- Not a sandbox first. The run is the product. Sandbox and Observer are modes, not the core. [Derived]
- Not multiplayer at launch. Co-op is stretch. [Spec]

---

## 2. Experience pillars

The specification names four pillars. Each one below has a concrete test, which is derived. A pillar is only met when its test can be passed.

### 2.1 Growth is the spectacle [Spec]

Every size jump must feel huge. The camera, the sound and the world's reaction all change.

- **Test:** at each tier transition, the camera distance and field of view change, the audio pitch changes, and the world reacts (new stratum, new prey, new hazards). A playtester can say which tier they are in without reading the HUD.

### 2.2 No two runs alike [Spec]

Strains, routes, mutations, ecosystems and events combine differently every run.

- **Test:** over 20 seeds, no two runs share the same route, mutation sequence and ecosystem outcome. Bots can measure divergence and report it. [Derived]

### 2.3 Readable danger [Spec]

Every death is fair, telegraphed and explained on the death screen.

- **Test:** every death shows a cause, a timeline and a heatmap. Every event is telegraphed 5–10 seconds ahead with audio and visual cues. Bot death causes can be traced to a specific rule. [Derived]

### 2.4 Consequences [Spec]

The world reacts to how you eat. Greed destabilizes ecosystems and draws hostile attention.

- **Test:** a run that over-hunts a stratum shows measurable ecosystem collapse and a rising Threat meter before the first event fires. [Derived]

---

## 3. The player's experience

### 3.1 The fantasy

The player is a small, hungry organism in an ocean it does not understand. Its growth is the story of its survival. Eventually that growth changes the universe around it. [Derived from the pitch]

### 3.2 The arc of a single run

A run is one life. It moves through stages, and each stage adds a new kind of pressure.

| Stage | Strata | What the player learns or faces | Pressure |
|---|---|---|---|
| Opening | Tier 1, Impact Site | Movement, eating small prey, the size gate, the first struggle | Starvation, small predators |
| Growth | Tier 1 end, Tier 2 Shallows | Struggles with mid-sized prey, bigger hunters, the first mutation choice, the first boss | Predation, the first Threat events |
| Middle | Tiers 3–6 | Open water, the Deep, the Coast, the Continent. Environment-bound mutations must be adapted at transitions. | Global Alert from Tier 5, storms, nets |
| Late | Tiers 7–9 | Civilization, planet, solar system. Human military responses. Heavy Threat events. | Military strikes, solar flares |
| Finale | Tier 10, Cosmos | The Great Attractor and the endings | Supernovae, the void |

[Spec for tiers, bosses and the Global Alert. The stage column is derived.]

### 3.3 The arc across many runs

- **Run 1:** learn the verbs, lose early, read the death screen.
- **Runs 2 to N:** Genetic Memory unlocks new strains, starting mutations, biome variants and morphs. The Rival recurs and adapts to what the player does most. Hostility levels raise the stakes. The codex fills.
- **Long term:** the secret ending, which is the end of the Rival's arc. [Spec]

---

## 4. The core loop

The game runs four loops, nested inside each other.

| Loop | Timescale | Steps |
|---|---|---|
| **Moment** | Seconds | Find prey. Read its size class (edible, risky, dangerous). Chase or avoid. Consume, or struggle. Digest. Grow. |
| **Stratum** | Minutes | Feed and explore. Watch the Threat meter and the food web. Decide when to leave. Pick the next biome on the route map. |
| **Transition** | At each tier and boss | Choose 1 of 3 mutations. Adapt environment-bound mutations. Play the tier-up cinematic. |
| **Meta** | Across runs | Genetic Memory, Hostility, the Rival, the codex, endings and achievements. |

[Spec for the loops' contents. The timescales are derived. The specification does not give a target run length. See Q4.]

---

## 5. Moment-to-moment play

### 5.1 Controls

M1 bindings, from `crates/game/src/input.rs`. The specification requires full rebinding (section 18).

| Action | Keyboard and mouse | Gamepad |
|---|---|---|
| Move (camera-relative) | WASD | Left stick |
| Look | Mouse | Right stick |
| Ascend | Space | Right trigger |
| Descend | Left Ctrl | Left trigger |
| Dash | Left Shift | South (A) |
| Consume (mash in struggles) | Left mouse | Right trigger 2 |
| Ability 1 | Q | West (X) |
| Ability 2 | E | North (Y) |
| Pause | Esc | Start |

The organism moves in three dimensions. Ascend and descend are part of the fiction of an ocean, not a separate mode. [Spec + M1 implementation]

### 5.2 Eating

- Prey is dragged into the body in a consumption animation. [Spec]
- A brief hit-stop plays on the meal. [Spec]
- The audio is pitched by the size of the organism. [Spec]
- Eaten mass enters a digestion queue. The queue converts to biomass at a fixed rate. [Spec]
- Above 150% of stomach capacity the organism is Gorged (section 7.5). [Spec]

### 5.3 Struggles

Prey between 0.8× and 1.2× the organism's size triggers a skill-based struggle. [Spec]

M1 implementation: a progress meter starts at 45%. Each mash adds progress. Resistance drains progress over time, and resistance scales with the size ratio. The struggle times out after 6 seconds. A failed struggle deals 10 damage. Rumble plays during the struggle.

Design intent [Derived]: the struggle is readable at a glance (the meter), the input is simple (mash), and the stakes are visible (integrity loss on failure). Skill comes from timing the commitment, not from memorizing a combo.

### 5.4 Chasing, fleeing and dashing

- Movement speed scales with size. Larger organisms are faster in absolute terms but slower relative to their own size. [Spec's locomotion model, implemented as speed ∝ radius^0.8]
- A dash gives a short burst of speed at an energy cost. The organism can use it to escape or to close a gap. [M1 implementation]
- Dangerous hunters pursue the organism. Fleeing and dashing are the answer. [Spec]

### 5.5 Gorging

Above 150% of stomach capacity the organism is Gorged. It moves more slowly and takes extra damage. The player can still eat, but must weigh the cost. [Spec]

### 5.6 Camera

- Distance, field of view and motion blur all scale with the organism's size. [Spec]
- The camera follows the organism and can be orbited with the mouse or right stick. [M1]
- Motion blur and camera shake can be turned off. [Spec, section 18]

### 5.7 Feel toolkit

| Tool | Behaviour | Source |
|---|---|---|
| Hit-stop | Virtual time pauses for a few real-time frames on large meals, bites and struggle results | [Spec + M1] |
| Consumption animation | Prey is dragged into the body | [Spec] |
| Audio pitched by size | Bigger organisms sound deeper | [Spec + M1] |
| Gamepad rumble | During struggles and impacts | [Spec] |
| Camera shake | Trauma-based, scaled by event, can be disabled | [M1] |
| Tier-up cinematic | Zoom-out of 3–5 seconds. Skippable. | [Spec] |

---

## 6. Growth and scale

### 6.1 The size model

The sim radius of an organism is the base radius times the cube root of its biomass. Speed scales with radius raised to 0.8. [M1 implementation, from `data/balance/metabolism.ron`]

| Biomass B | Radius (base 0.25 at B = 1) | Notes |
|---|---|---|
| 1 | 0.25 | Start of Tier 1 |
| 8 | 0.50 | |
| 150 | 1.33 | Start of Tier 2 |
| 4,000 | 3.97 | End of Tier 2 (M1 values) |

[Derived: the table is computed from the M1 formula.]

Gameplay uses ratios between the organism and its targets (0.8×, 1.2×). That makes the rules scale-free. [Derived]

**Open problem [Derived]:** with radius ∝ B^(1/3), a literal ten-stratum size ladder, microbe to universe, would need a biomass range beyond anything playable. The presentation scale must therefore be decoupled from literal biomass. The planned answer is a per-stratum presentation scale using `big_space` grids, with the gameplay rules kept on ratios. This needs a decision before Tiers 4 to 10 are designed. See Q5.

### 6.2 Biomass ladder

| Tier | Start B | Complete B | Source |
|---|---|---|---|
| 1 | 1 | 150 | M1 values |
| 2 | 150 | 4,000 | M1 values |
| 3–10 | Not defined | Not defined | [Open] Q5 |

A tier transition triggers when the biomass threshold is reached. [Spec]

### 6.3 Growth stages

- The creature's body is regenerated as it grows and as it mutates. [Spec]
- Regeneration happens asynchronously on `AsyncComputeTaskPool`, so the frame does not stall. [Spec]
- A new mesh is built when the growth stage changes. The M1 stage is derived from the base-2 logarithm of biomass. [M1 implementation]
- Lobes are added as the body grows. Mutations change the silhouette. [Derived]

### 6.4 The tier transition

- A zoom-out cinematic plays for 3 to 5 seconds. It can be skipped. [Spec]
- The next stratum loads asynchronously during the cinematic. [Spec]
- The stratum transition must take under 3 seconds, and it is hidden behind the zoom-out. [Spec]
- A stratum title card appears after arrival. [M1]

**Conflict in the specification [Open] Q6:** the cinematic is 3–5 seconds and the transition is under 3 seconds. The reading used here is that the load (under 3 seconds) happens inside a cinematic of 3 to 5 seconds. M1 uses 3.6 seconds, with the world swapped at 60% through the cinematic.

---

## 7. Survival systems

All numbers in this section live in `data/balance/*.ron` and can be changed live in the debug panel. [Spec]

### 7.1 Player state

| Stat | Meaning | Range |
|---|---|---|
| Biomass (B) | Size and growth | Unbounded upward |
| Energy (E) | Fuel, drained by upkeep | 0–100 [Spec] |
| Integrity | Health. Death at 0. | [Spec] |
| Stomach | Digestion queue fill, relative to capacity | Gorged above 150% [Spec] |
| Threat | Stratum-level pressure (section 8.2) | 0–100 |
| Instability | Mutation-mixing risk (section 8.4) | 0–100 |

### 7.2 Upkeep

Energy drains at `k × B^0.75` per second. This is Kleiber's law. The coefficient `k` depends on the stratum and on the strain. [Spec]

M1 values: Tier 1 `k` = 0.15, Tier 2 `k` = 0.42. The Devourer's upkeep multiplier is 1.0. [M1 data]

### 7.3 Starvation

- When E is 0, Integrity drains. [Spec]
- When Integrity is 0, the organism dies. [Spec]
- M1 values: starvation drains 6 Integrity per second. Integrity regenerates at 1.5 per second while E is at least 40. [M1 data]

### 7.4 Size gate [Spec]

| Target size relative to the organism | Class | Effect |
|---|---|---|
| At or below 0.8× | Edible | Safe to eat. Shown as edible. |
| Above 0.8× up to 1.2× | Risky | Triggers a skill-based struggle. |
| Above 1.2× | Dangerous | Cannot be eaten. Hunts the organism (see the deviation in section 28). |

The classes are shown with colour and with shape: a torus marks risky prey and a cone marks dangerous prey. [M1]

### 7.5 Digestion and Gorging [Spec]

- Eaten mass enters a queue.
- The queue converts to biomass at a fixed rate.
- Above 150% of capacity, the organism is Gorged. It moves at 0.6× speed and takes 1.5× damage. [M1 values]

### 7.6 Death [Spec]

Every death has a recorded cause. The cause is one of:

- **Starvation**
- **Predation**: killed by a named species
- **Struggle injury**: killed in a struggle with a named species
- **Event**: killed by an environmental event (planned; not yet in M1)

The death screen shows the cause in plain language, the run timeline, a heatmap of where the organism spent its time in its final stratum, and the Genetic Memory earned. [Spec]

---

## 8. Consequence systems

### 8.1 The ecosystem

Each stratum runs a simplified food web in `game_core`, using a Lotka–Volterra-style population model. [Spec]

Rules [Spec]:

- Over-hunting collapses a species.
- A collapsed species' predators starve and turn on the organism.
- Leaving breeders alive lets populations recover. Restraint is rewarded.
- Ecosystem collapse raises the Threat meter.

Derived design:

| Ecosystem state | Meaning | Player-visible sign |
|---|---|---|
| Thriving | Populations at or near capacity | Normal density of prey |
| Stressed | A species is declining | Prey thins in one area |
| Collapsing | A species is near zero | Predators turn aggressive, ambient bioluminescence dims |
| Collapsed | A species is gone | The stratum's Threat rises. Recovery needs breeders left alive. |

Requirements [Derived]:

- Per-species growth, predation and carrying-capacity values live in RON.
- The model is deterministic for a given seed and input stream (section 22.3).
- Only agents near the organism exist as full ECS entities. Distant populations are aggregate data. [Spec, swarm rendering]

M1 status: species have fixed population counts that respawn after deaths. There is no population dynamics yet. That is M2.

### 8.2 Threat

Each stratum has a Threat meter from 0 to 100. [Spec]

**Feeders** [Spec]:

- Notoriety: biomass gained per minute above a threshold
- Time spent in the stratum
- Ecosystem collapse
- Seeded random events

**Escalation** [Spec]: at 33, 66 and 100, escalating events fire from the stratum's event table. The table contains: red tide, predator frenzy, storms, nets, military strikes, solar flares and supernovae.

**Telegraphs** [Spec]: every event is telegraphed 5–10 seconds ahead with audio and visual cues. Readable danger depends on this.

**Decay** [Spec]: Threat decays while the organism hides, rests or uses stealth.

**Suggested event mapping** [Derived, to confirm in Q7]:

| Event | Suggested stratum | Telegraph idea |
|---|---|---|
| Red tide | Shallows, Deep | Water tints; a low chorus rises |
| Predator frenzy | Any ocean stratum | Agents swarm toward the organism |
| Storms | Coast | Surface light flickers; current pushes |
| Nets | Coast, Civilization | Shadows descend in a grid |
| Military strikes | Civilization, Planet | Sirens, then a shockwave |
| Solar flares | Solar System | Light floods the scene; the shell glows |
| Supernovae | Cosmos | A rising hum; a blinding flash |

### 8.3 Global Alert

From Tier 5 onward, a Global Alert level carries humanity's escalating response between strata. [Spec]

**[Open] Q8:** the levels, their triggers and their effects are not specified. [Derived] The Containment ending (section 13.4) is the natural end of this track.

### 8.4 Genetic Instability

Mixing mutation branches raises Instability. [Spec]

| Instability | Effect |
|---|---|
| Below 50% | No effect |
| 50% | Mutations randomly misfire |
| 100% | Rejection: the organism loses a random mutation and 25% of its biomass |
| Second rejection in one run | Fatal |

[Spec]

---

## 9. Mutations

### 9.1 Rules [Spec]

- At each tier transition and at each boss kill, the player picks 1 of 3 mutations.
- There are five branches: **Predator, Absorber, Swarm, Carapace, Psionic**.
- Every mutation has a cost as well as a benefit.
- Picking 4 mutations in one branch unlocks that branch's capstone ability.
- Two specific capstone pairs unlock hidden hybrid mutations. **[Open] Q9:** the pairs are not named.
- Environment-bound mutations (gills, pressure organs, vacuum shells) must be adapted at transitions, or they become liabilities. [Spec]

### 9.2 Data-driven design [Spec]

Mutations are data. Effects, costs and tags are defined in RON. A generic ability and effect system in `game_core` applies them. No mutation is a special case in code. [Spec]

Derived card anatomy, for each mutation:

- Name and branch
- Benefit (the effect)
- Cost (the trade-off)
- Tags (used by the effect system and by hybrid rules)
- Instability contribution (how much it adds when mixed)
- Environment binding, if any, with the adaptation rule at each transition
- Capstone progress counter

### 9.3 Branch themes [Derived suggestions, to confirm in Q9]

| Branch | Suggested theme | Example benefit | Example cost |
|---|---|---|---|
| Predator | Hunting power | Longer bite reach | Higher upkeep |
| Absorber | Digestion | Higher energy yield per mass | Slower Gorged recovery |
| Swarm | Splitting | Extra bodies that absorb damage | Partial losses on hit |
| Carapace | Defence | Higher maximum integrity | Lower top speed |
| Psionic | Perception | Earlier Threat warnings | Higher Threat from notoriety |

### 9.4 Environment-bound examples [Derived]

| Mutation | Adaptation point | Why it becomes a liability if not adapted |
|---|---|---|
| Gills | Transition out of an oxygen-poor stratum | Suffocation in new environments |
| Pressure organs | Entering the Deep | Crushing in shallow water |
| Vacuum shells | Entering Space strata | Overheating in atmosphere |

---

## 10. Strains

A strain is a playable starting organism. Each strain changes the core verb of the game. [Spec]

Only the Devourer is in M1. Genetic Memory unlocks the rest. [Spec, M1 data]

### 10.1 Devourer (starter) [Spec]

- **Core verb:** direct consumption.
- **Feel:** fast and aggressive.
- **M1 tuning:** speed factor 1.15, acceleration 6 per second, dash ×2.2 for 0.35 seconds, dash cooldown 1.6 seconds, dash cost 3% energy, bite reach 1.05 organism radii. [M1 data]

### 10.2 Swarm Strain [Spec]

- **Core verb:** control a hive of many small bodies.
- **Biomass:** the body count. Losses are partial.
- **Rendering:** the swarm uses the boids compute system, the same one used for ambient swarms. [Derived]
- **Open:** the partial-loss formula, how swarm bodies become ECS entities, and how the hive is shown in the HUD. [Open] Q10

### 10.3 Mycelial Bloom [Spec]

- **Core verb:** spread a network across terrain.
- **Feel:** consumes through territory. Rewards strategic expansion.
- **Derived:** territory is a grid or graph in `game_core`. Expansion covers ground the player can see on the seafloor or cave walls.
- **Open:** how the network interacts with Threat and with predators. [Open] Q10

### 10.4 Mimic [Spec]

- **Core verb:** assimilate prey to copy its abilities and appearance.
- **Feel:** stealth. Keeps Threat low.
- **Derived:** copied appearance needs a material and mesh swap. A limit on the number of copies keeps the strain from becoming a catalogue.
- **Open:** the assimilation rules and the limits. [Open] Q10

### 10.5 Crystalline Lattice [Spec]

- **Core verb:** feed on energy (heat, light, radiation) as well as mass.
- **Feel:** thrives inside hazards that kill other strains.
- **Derived:** needs hazard fields in the terrain, such as vents or radiation zones. The Hydrothermal Field biome in Tier 1 is an obvious first place for this.
- **Open:** the energy sources and how they map onto biomes. [Open] Q10

### 10.6 Strain milestones [Spec]

| Strain | Milestone |
|---|---|
| Devourer | M1 |
| Swarm, Mimic | M4 |
| Mycelial, Crystalline | M5 |

---

## 11. The world

### 11.1 Strata

A stratum is a tier of the run. Each tier is its own world region, built with `big_space` grids for scale. [Spec]

| Tier | Stratum | Biome variants | Apex boss | Milestone |
|---|---|---|---|---|
| 1 | Impact Site | Crater Seafloor, Hydrothermal Field | Colony Mat | M1 grey-box; M2 visual |
| 2 | Shallows | Coral Reef, Kelp Forest, Mangrove | Reef Warden | M1 grey-box; M2 visual |
| 3 | Open Ocean | Pelagic Blue, Arctic Shelf | Great White Matriarch | M2 visual |
| 4 | Deep | Abyssal Plain, Trench | Colossal Squid | M4 |
| 5 | Coast | Beach, Estuary, Ice Coast | Storm-front (living weather) | M4 |
| 6 | Continent | Rainforest, Savanna, Taiga | Ecosystem Alpha | M4 |
| 7 | Civilization | Port City, Megacity, Military Zone | Containment Mech | M4 |
| 8 | Planet | Crust, Core, Moon | Planetary Defense Grid | M5 |
| 9 | Solar System | Inner Planets, Gas Giants, the Sun | Corona | M5 |
| 10 | Cosmos | Galaxy, Galactic Core, the Void | The Great Attractor | M5 |

The specification's column for biome variants says "ship 1, add more later." Read literally, each tier ships with one biome variant, and the rest come after launch. That conflicts with the route map, which offers two or three options. **[Open] Q11.**

### 11.2 Biome identity

The specification names the biomes. The table below suggests an identity for each one. These suggestions are [Derived] and need an art and design brief.

| Biome | Suggested identity |
|---|---|
| Crater Seafloor (T1) | Impact glass, shocked rock, a glowing meteor scar. Built in M1. |
| Hydrothermal Field (T1) | Black smokers, chemosynthetic mats, warm haze |
| Coral Reef (T2) | Bommies, fans and sea fans, caustic light from above. Built in M1. |
| Kelp Forest (T2) | Vertical kelp columns, dappled light, swaying forests |
| Mangrove (T2) | Root tangles, brackish murk, low visibility |
| Pelagic Blue (T3) | Endless open water, deep blue gradient, distant shapes |
| Arctic Shelf (T3) | Ice ceilings, cold blue-white light, sea ice shadows |
| Abyssal Plain (T4) | Dark, flat, sparse, bioluminescent points |
| Trench (T4) | Steep walls, extreme pressure cues, deepest darkness |
| Beach (T5) | Surf line, sun shafts through the surface, shore debris |
| Estuary (T5) | Silt plumes, mixed fresh and salt water, reeds |
| Ice Coast (T5) | Ice shelves, meltwater plumes, storm light |
| Rainforest (T6) | Canopy shadow, dense green-brown colour, humid haze |
| Savanna (T6) | Open plains, dry grass, long shadows |
| Taiga (T6) | Conifer shadow, cold bright light, snowfall |
| Port City (T7) | Harbour lights, ships, shipping containers, nets |
| Megacity (T7) | Towers, neon, roads, heavy human presence |
| Military Zone (T7) | Fences, searchlights, armour and sirens |
| Crust (T8) | Rock strata, fault lines, lava glow |
| Core (T8) | Iron heat, pressure, molten light |
| Moon (T8) | Grey dust, crater rims, the full Earth in the sky |
| Inner Planets (T9) | Scorched rock, the Sun's glare |
| Gas Giants (T9) | Banded clouds, storms, ring systems |
| The Sun (T9) | Plasma, flares, the blinding surface |
| Galaxy (T10) | Spiral arms, star fields, dust lanes |
| Galactic Core (T10) | Dense stars, a bright central mass, lensing |
| The Void (T10) | Near-empty space, faint cosmic background, lensing |

### 11.3 The route map [Spec]

After each tier, the player picks the next biome from 2 to 3 options. Each option has:

- its own prey
- its own hazards
- its own mutation pool
- its own reward type: **biomass**, **rare mutation** or **memory fragment**

[Derived] The route map is a small graph, with the options visible before the choice is made. Each option shows its reward type as an icon and a text label, not colour alone.

### 11.4 Procedural generation [Spec]

- The run seed drives layout, prey distribution, hazards and points of interest.
- Same seed means same world.
- Seeds are shareable.

[Derived] Shared seeds only stay valid if the generator is versioned. A seed must carry the generator version it was made with, or a patch will silently change the worlds that friends share.

### 11.5 Living ecosystems inside a stratum [Spec]

- Populations are per stratum and persist for the length of a visit.
- Endless Bloom uses a respawning ecosystem (section 14).

---

## 12. Creatures and prey

### 12.1 The M1 roster

Radii are in sim units. The organism's base radius is 0.25 at biomass 1. Counts are the populations kept alive in each arena. Source: `data/balance/strata.ron`.

**Tier 1, Impact Site (Crater Seafloor)**

| Species | Radius range | Count | Behaviour | Sense | Shape |
|---|---|---|---|---|---|
| Glass diatom | 0.07–0.13 | 150 | Drifter | 6 | Blob |
| Copepod | 0.14–0.26 | 60 | Skittish | 9 | Elongated |
| Brine shrimp | 0.30–0.55 | 30 | Skittish | 10 | Elongated |
| Arrow worm | 0.45–0.70 | 8 | Hunter | 7 | Spiky |
| Comb jelly | 0.70–1.00 | 14 | Drifter | 6 | Jelly |
| Lanternfish fry | 1.10–1.50 | 4 | Hunter | 7 | Fish |

**Tier 2, Shallows (Coral Reef)**

| Species | Radius range | Count | Behaviour | Sense | Shape |
|---|---|---|---|---|---|
| Reef shrimp | 0.45–0.80 | 70 | Skittish | 8 | Elongated |
| Damselfish | 0.90–1.40 | 50 | Skittish | 9 | Fish |
| Sea cucumber | 1.00–1.80 | 20 | Grazer | 4 | Elongated |
| Parrotfish | 1.60–2.50 | 26 | Skittish | 9 | Fish |
| Moray | 2.00–3.00 | 6 | Hunter | 8 | Elongated |
| Sea turtle | 2.80–3.60 | 10 | Grazer | 6 | Blob |
| Reef shark | 3.60–4.60 | 4 | Hunter | 10 | Fish |

"Sense" is perception range, in the organism's own radii. [M1 data]

### 12.2 Behaviours [Spec, M1 data]

| Behaviour | Meaning |
|---|---|
| Drifter | Wanders slowly |
| Skittish | Flees bigger eaters |
| Hunter | Hunts the organism when it is more than 1.2× the hunter's size |
| Grazer | Wanders and ignores everything |

**Deviation:** the specification says that every target above 1.2× hunts the organism. M1 only hunts with the Hunter behaviour. Large Drifters, Skittish and Grazers never hunt. This needs a decision (Q12).

### 12.3 Size tinting and readability [Spec]

- Prey is tinted by size class: edible, risky, dangerous.
- Tints are colour-blind-safe, and every class also has a shape or pattern. Colour is never the only signal.

M1: a palette chosen for colour-blind safety, with torus markers on risky prey and cone markers on dangerous prey.

### 12.4 Swarms [Spec]

- Ambient swarms are a WGSL compute shader (boids) writing to an instance buffer.
- They render with a custom instanced material.
- Swimming is animated procedurally in the vertex shader.
- Only agents near the organism become full ECS entities, with physics and AI. Distant populations are aggregate data.

### 12.5 Apex bosses [Spec]

Each tier has an apex boss (section 11.1). [Open] Q13: boss mechanics are not specified. The first boss arrives in M2.

[Derived] A boss kill grants a mutation choice (section 9.1). That ties the boss to the progression loop.

---

## 13. Progression and replayability

### 13.1 Genetic Memory [Spec]

Each run earns Genetic Memory points. Points unlock:

- strains
- starting mutation options
- biome variants
- cosmetic morphs

Genetic Memory never gives raw stat boosts. [Spec]

[Derived] Testable rule: a player who has unlocked everything can still be beaten by a player who has unlocked nothing, and the stat ceilings are the same. Unlocks add options, not power. The point economy is [Open] Q14.

### 13.2 Hostility levels 1–20 [Spec]

- Hostility is a set of stackable difficulty modifiers. The levels run from 1 to 20.
- Each level adds rewards and has its own leaderboard.
- Acceptance tests use Hostility 0, the base game with no modifiers. [Spec]

[Open] Q15: what each modifier does.

### 13.3 The Rival [Spec]

- An older organism from a previous meteor.
- It recurs across runs.
- It adapts to the player's most-used strategies.
- Its arc unlocks the secret ending.

[Derived] Adaptation uses local statistics only, such as strains used and mutation branches picked. Nothing is uploaded unless the player opts in. [Open] Q16 covers the adaptation model and the arc beats.

### 13.4 Endings [Spec]

| Ending | Specified condition | Derived reading (to confirm) |
|---|---|---|
| 1. Consume the Universe | Not specified | Completing Tier 10 (the Great Attractor) |
| 2. Seed | Spread to other worlds | A reproductive path in the late game. Trigger open. |
| 3. Symbiosis | Requires low ecosystem collapse | Finished with ecosystems at or below an agreed collapse threshold |
| 4. Containment | Stopped by humanity | The end of the Global Alert track (section 8.3) |
| 5. Secret | Meet the sender | The completion of the Rival's arc |

[Open] Q17: thresholds and exact triggers.

### 13.5 Codex, bestiary and achievements [Spec]

- The codex and bestiary cover species, mutations and memory fragments.
- About sixty achievements.
- Achievements are disabled in Mutator Lab (section 14).
- Assist mode disables leaderboards (section 18).

---

## 14. Game modes

| Mode | Specified content | Notes and derived requirements |
|---|---|---|
| **Ascension** | The full run, Tiers 1 to 10, with multiple endings | The main game. M1 has Ascension only. |
| **Endless Bloom** | One stratum. A respawning ecosystem. Threat escalates forever. Score attack. | Needs a score model and an escalation curve. [Open] Q18 |
| **Daily Seed / Weekly Mutation** | A fixed seed and fixed mutators. One attempt. Steam leaderboards with a local fallback. | Needs the generator version (section 11.4). Local leaderboard first (M3), Steam later (M6). |
| **Speedrun** | An in-game timer with splits and ghost replays. Categories: Any% Random Seed, Fixed Seed, All Bosses. | Ghosts need deterministic replay (section 22.3). |
| **Extinction Events** | 30 hand-designed challenge scenarios. Example: "Consume the reef without collapsing any species." | Each scenario needs an objective, a win check and a readable failure reason. |
| **Mutator Lab** | Custom runs with any mutators, strains and balance overrides. Achievements are disabled. | Overrides must never reach the main leaderboards. |
| **Sandbox / Observer** | Spawn creatures. Control time and scale. Includes photo mode. | Needs time-scale and free-camera controls. Photo mode needs a capture tool. |
| **Versus and Co-op** (stretch) | 2–8 organisms in one stratum, using `lightyear` and Steam networking. | Needs an authority model. [Derived] see section 22.3. |

---

## 15. Narrative

- Storytelling is environmental only. Nothing is delivered as dialogue. [Spec]
- Memory fragments come from consuming specific species, artifacts or bosses. They slowly reveal where the organism came from. [Spec]
- Humanity's broadcasts escalate in three stages: science reports, military orders, then prayers. [Spec]

[Derived] Production structure:

- Each memory fragment has an ID, a tier, a source (species, artifact or boss) and a text entry in the codex.
- Each broadcast is a Fluent message keyed to the Global Alert level, so the same story works in every language.
- A story bible is kept in `docs/`. It is not written yet. [Open] Q19

---

## 16. Presentation and art direction

### 16.1 Art direction [Spec]

Fidelity comes from lighting, atmosphere and motion, not from polygon counts:

- bioluminescence
- volumetric light shafts
- fog
- caustics
- particles
- refraction
- glowing cosmic scenes

The look has to scale from an M1 to a high-end PC. A cohesive art direction matters more than photorealism.

### 16.2 Rendering by preset [Spec]

**All presets:**

- PBR: `StandardMaterial` plus custom extended materials
- AgX or TonyMcMapface tonemapping
- Bloom, SSAO, distance fog
- Environment-map lighting
- GPU instancing, visibility ranges and LOD, frustum culling

**Medium and up:** volumetric fog (`FogVolume`), screen-space reflections (SSR), TAA, depth of field.

**High and up:** irradiance volumes for global illumination, more shadow cascades, motion blur, denser particles.

**Ultra (optional):** Bevy's experimental ray-traced lighting. Only if the pinned Bevy version supports it, and only on GPUs with hardware ray tracing.

### 16.3 Upscaling [Spec]

- Render scale plus TAA on every preset.
- DLSS or FSR only if a maintained integration exists for the pinned Bevy version. As of M0, Bevy 0.20 ships DLSS behind a feature. FSR has no maintained integration. [Backlog]

### 16.4 Custom post-processing [Spec]

Written as render-graph nodes in WGSL:

- underwater caustics
- depth-based colour absorption
- heat distortion
- cosmic gravitational lensing

### 16.5 Environments [Spec + M1]

- Each stratum has its own environment map, used for lighting and as the skybox.
- Underwater environments use a procedural gradient (built in M0).
- Cosmic strata (Tiers 8–10) need their own environment maps and lensing.

### 16.6 Materials and shaders [Spec + M1]

- The creature uses `ExtendedMaterial`: a bioluminescent skin with pulse and wobble. [M1]
- Bevy's PBR library is WESL, so material shaders that import Bevy modules are `.wesl`. [M0 finding]
- Emission is kept out of camera exposure, because the exposure weight is set to zero. [M0 finding]

### 16.7 The creature [Spec]

- The body is a metaball field turned into a mesh with marching cubes.
- It is regenerated asynchronously on `AsyncComputeTaskPool` as it grows and mutates.
- A vertex shader drives the pulse and the wobble.
- [M1] The mesh is watertight at every growth stage, which is tested.

### 16.8 HUD and visual readability [Spec]

HUD elements: **Biomass, Energy, Integrity, Threat, Instability.**

- Status tags carry text (for example [HUNGRY] and [GORGED]). Colour is never the only signal. [M1]
- The HUD sits on a translucent backdrop so labels stay readable over bright sand and close prey. [M1]

---

## 17. Audio

- **Adaptive score.** The music layers up as Threat rises. [Spec]
- **Underwater filtering.** A low-pass filter applies underwater. [Spec]
- **Pressure rumble.** Deep strata add a low rumble. [Spec]
- **Space.** In space there is no air, so sound is felt as low-frequency vibration rather than heard. [Spec] [Derived] This is implemented as a vibration channel, using haptics and low-frequency content.
- **Engine.** `kira`, through `bevy_kira_audio`. Mix buses. The specification asks for buses but does not list them. The list used here (music, effects, ambience, vibration) is [Derived].
- **Consumption audio.** Pitched by the organism's size. [Spec]
- **Synthesized fallbacks.** Every sound has a synthesized fallback, so the game builds and runs without downloaded assets. [Spec] M1 has six synthesized effects: gulp, crunch, bite, grapple, tier-up swell and death.
- **Captions.** Subtitles and SFX captions. [Spec]

M1 status: sound effects only, with captions. No adaptive score, no buses and no underwater filter yet. Those are M2. No audio device was available in the M1 build environment, so the effects have not been listened to.

---

## 18. UI, UX and accessibility

### 18.1 Accessibility requirements [Spec]

- **Full rebinding** of every action.
- **Colour-blind-safe indicators** that use shape and pattern, never colour alone.
- **Subtitles and SFX captions.**
- **Toggles** for camera shake and motion blur.
- **UI scaling.**
- **Assist-mode game-speed slider.** It disables leaderboards.

M7 includes a full accessibility audit. [Spec]

### 18.2 Death screen [Spec]

- Cause of death
- Run timeline
- Heatmap
- Genetic Memory earned

M1 has all four. Genetic Memory shows 0 until M3.

### 18.3 Controller support [Spec + Derived]

- The launcher is fully navigable with a controller. [Spec]
- In-game, every action can be bound to a gamepad. [Derived from the rebinding requirement]
- Gamepad glyphs are localized. [Derived; see Backlog]

### 18.4 Menus

- Main menu, pause menu, settings, codex, mode select, route map, death screen, victory screen. [Derived list]
- Continue appears on the main menu while a run save exists. [M1]
- Retry and Main menu on the end screen. [M1]

---

## 19. Localization

- Every string is loaded from Fluent (`.ftl`) files. [Spec]
- The game is ready for ten or more languages. [Spec]
- The M7 pass covers translation review. [Spec]

[Derived] Requirements:

- No sentence built by concatenating strings, because grammar differs by language. A death message that inserts a species name mid-sentence is a known example. Use Fluent placeholders and grammatical cases instead.
- Plural and gender rules are handled by Fluent, not by code.
- Font coverage is checked per language. An M0 bug showed that a missing glyph renders as a box.
- Text must fit with longer translations. Layouts are tested at the longest expected language.

---

## 20. Launcher

A native Rust application in the same workspace, built with `eframe`/`egui`. It is a small binary and starts in under one second. [Spec]

### 20.1 System scan [Spec]

- GPU name, vendor, backend and driver, from wgpu adapter information.
- CPU, core count, RAM and free disk, via `sysinfo`.
- Display resolution and refresh rate, via `winit`.
- Assigns an automatic preset.
- Warns when the GPU driver looks outdated, with a link to the vendor's driver page.

### 20.2 Benchmark [Spec]

- Runs the game with `--benchmark`, which writes a results file.
- Shows average fps and 1% low fps.
- Recommends settings.

### 20.3 Shader warm-up [Spec]

- Runs the game with `--warmup`. Every render pipeline and material combination is built offscreen before the first play.
- Uses a wgpu pipeline cache where the backend supports one.
- Runs on first launch, after updates and after driver changes.
- Shows a progress bar.

### 20.4 Settings editor [Spec]

Writes the RON configuration the game reads:

- display mode, resolution, render scale, upscaler, frame cap, VSync
- per-setting quality overrides
- audio device
- rebinding
- accessibility
- language

### 20.5 Integrity [Spec]

- Verifies files against a SHA-256 manifest.
- The manifest is signed with Ed25519.
- Repairs damaged files.

### 20.6 Updates (non-Steam builds only) [Spec]

- Signed delta patches (bsdiff plus zstd), with rollback.
- Patch notes are bundled locally, so the launcher works offline.

### 20.7 Mods tab [Spec]

- Enable or disable data and asset mods.
- Set load order.
- Detect conflicts.

### 20.8 Crash handling [Spec]

- The game installs a panic hook and a minidump writer.
- On an abnormal exit the launcher offers an opt-in crash report.
- It also offers Safe Mode relaunch: Low preset, mods off.

### 20.9 Diagnostics [Spec]

One click exports logs, configuration and system information as a zip.

### 20.10 Launch options [Spec]

- "Launch directly next time."
- Backend choice: D3D12 or Vulkan on Windows, Metal on Mac.
- Custom arguments.
- After launch: minimize or close.

### 20.11 Steam [Spec]

When the game runs under Steam, it defaults to "launch directly" and hides the updater.

### 20.12 Launcher UI [Spec]

- Fully navigable with a controller.
- Localized.

M0 delivered a skeleton: the play button, presets, backend, custom arguments, after-launch behaviour, Safe Mode after an abnormal exit, a basic system scan and localized strings. M2 brings v1 (system scan, settings editor, shader warm-up, benchmark). M6 completes the launcher.

---

## 21. Platforms and performance targets

### 21.1 Presets [Spec]

| Preset | Reference hardware | Target |
|---|---|---|
| Low | Apple M1 8 GB, or GTX 1060 6 GB | 1080p (render-scaled), 60 fps |
| Medium | GTX 1660 Super, or RX 5600 XT | 1080p native, 60 fps |
| High | RTX 3060, or RX 6600 XT | 1440p, 60 fps |
| Ultra | RTX 4070 or better | 4K (upscaled), 60+ fps |

### 21.2 Other targets [Spec]

- **Memory:** under 3.5 GB total at Low.
- **Frame pacing:** no frame spike above 50 ms after shader warm-up.
- **Reporting:** always report average fps and 1% lows.
- **Cold start:** to the main menu in under 10 seconds.
- **Stratum transition:** under 3 seconds, hidden behind the zoom-out.
- **Launcher start:** under 1 second.

### 21.3 Measured so far

M1 numbers are on the Low preset, and are the only measured performance data so far.

- The Mac has not been measured yet. The target is 60 fps average at Low, with memory under 3.5 GB. [Open until run]
- In the build container, the software renderer runs at about 2.3 fps, which says nothing about GPU performance. Memory there is about 928 MiB after a 30-minute soak. [M1]

### 21.4 Platform packaging [Spec]

- Build with `tools/build.sh [mac|windows|linux] [debug|release]`.
- macOS builds locally.
- Windows and Linux builds run in CI on GitHub Actions runners.
- Optional local cross-compile to Windows with `cargo-xwin`.
- Code signing and notarization steps are documented in `docs/Release.md`. They are done later. [Spec]

---

## 22. Technical architecture

### 22.1 Workspace [Spec]

| Crate | Responsibility | Depends on Bevy |
|---|---|---|
| `game_core` | Pure simulation: rules, balance data, the run sim, bots, saves, localization, manifest, perf statistics. Testable headless. | Optional (`bevy` feature) |
| `game` | The playable application: rendering, input, audio, UI, creatures, ecosystem, strata | Yes |
| `launcher` | `eframe`/`egui` launcher | No |
| `tools` | `agtool`: texture conversion, data validation, asset verification, credits | No |

Other top-level directories: `data/` (RON and JSON), `assets/` (locale, manifest, fetched assets), `shaders/` (WESL and WGSL), `tools/` (build and fetch scripts), `docs/`. [Spec]

### 22.2 Plugins and states [Spec]

**One plugin per domain:** Core, Creature, Ecosystem, Strata, Mutation, Threat, AI, UI, Audio, Save, Mod, Online.

**States:** `AppState` (Boot, MainMenu, InRun, Tooling) and `RunState` as a sub-state of InRun (Playing, Paused, TierTransition, Dead, Victory).

**Communication:** cross-plugin communication uses events and messages only. No plugin calls another's systems directly. [Spec]

**Simulation:** runs in `FixedUpdate`. [Spec] The current build runs at 64 Hz.

### 22.3 Determinism [Derived]

Several required features depend on the simulation being deterministic:

- shareable seeds
- Daily Seed leaderboards
- Speedrun ghost replays
- balance bots, which must reproduce a failure from its seed
- co-op, if built, which needs a clear authority model

The rule: for a given seed, generator version and input stream, `game_core` produces the same run. The M1 bots already meet this on one machine, and a test checks it. Two things are needed beyond that: a fixed rule for the input stream, and a check across platforms.

### 22.4 Physics, floating origin and particles [Spec]

- **Physics:** `avian3d`.
- **Floating origin:** `big_space`, a grid-based floating origin for huge coordinates.
- **Particles:** `bevy_hanabi` for GPU particles, including marine snow.

### 22.5 Input and debug [Spec]

- **Input:** `leafwing-input-manager`, with full rebinding.
- **Debug (debug builds only):** `bevy_egui` and `bevy-inspector-egui`.

### 22.6 Data and noise [Spec]

- `serde`, RON and JSON, with `bevy_common_assets`.
- Asset hot reload through the `file_watcher` feature.
- Procedural noise with `noise` or `fastnoise-lite`.
- CPU parallelism with `rayon`.

### 22.7 Save system [Spec]

- Versioned saves, using serde, with migrations.
- Run state and meta-progression stored separately.
- Autosave at every tier.

M1 has run saves only, one per tier, deleted on death, victory or abandonment. Meta-progression is M3.

### 22.8 Modding [Spec]

- Data mods (RON and JSON) and asset folders.
- Documented in `docs/Modding.md`.
- Managed in the launcher's Mods tab (section 20.7).

[Derived] Mod data goes through the same validators as base data. A mod that breaks a rule is rejected with a message that names the file and the field.

### 22.9 Steam [Spec]

- `steamworks`, behind a trait, so non-Steam builds work.
- Achievements, cloud saves and leaderboards in M6.

### 22.10 Netcode (stretch) [Spec]

- `lightyear`, for versus and co-op.

### 22.11 Rendering and UI [Spec]

- Rendering through Bevy's built-in renderer, with custom extended materials and render-graph nodes.
- UI is `bevy_ui`, with a small themed widget layer. [Spec]

---

## 23. Content and asset pipeline

### 23.1 Principles [Spec]

- Assets are imported at build time and never fetched at runtime.
- Every asset has a procedural fallback: a generated mesh, a shader or a synthesized sound. The game builds even if a download fails.
- Every asset is tracked in `assets/manifest.json` with its source URL, author, licence and hash.
- `CREDITS.md` is generated automatically from the manifest.

### 23.2 Sources [Spec]

- Poly Haven (HDRIs, textures, models)
- ambientCG (PBR textures)
- Quaternius and Kenney (models and animations)
- Freesound, filtered to CC0 (sound effects)

Downloads use the CC0 or permissive licences only.

### 23.3 Resolutions [Spec]

- Textures at 1K or 2K by default.
- 4K textures go in an optional **Ultra texture pack**.
- The base asset set stays under 5 GB.

### 23.4 Formats [Spec]

- Textures converted to KTX2 with GPU compression.
- Models as glTF.

### 23.5 Pipeline stages

1. `tools/fetch_assets.py` downloads from the sources.
2. `agtool texconv` converts textures.
3. The manifest and credits are written.
4. CI runs `agtool verify-assets` (licence allowlist, HTTPS sources, unique IDs, hashes) and `agtool credits --check`. [M0]

Status: the pipeline exists. No downloads have run yet. The manifest hashes are null until the first fetch on the Mac. [STATUS]

---

## 24. Quality, testing and verification

### 24.1 Unit tests [Spec]

- `cargo test` covers `game_core`.
- `clippy` is clean.
- `rustfmt` is enforced in CI.

### 24.2 Headless balance bots [Spec]

- Built on `MinimalPlugins`, running the simulation at an accelerated fixed timestep.
- **1,000 runs locally, and 10,000 in CI per balance change.**
- Reports the death-cause distribution, run length and win rate **per strain and per Hostility level**.
- Acceptance: no single death cause above 40% at Hostility 0. [Spec]

### 24.3 Visual verification [Spec]

- `--screenshot <scene> <preset> <out.png>` renders a frame to disk.
- Used to check visual work before asking the user to look at it.
- Run scenes (`run-tier1`, `run-tier2`, `death`) are driven by the bots. [M1]

### 24.4 Debug tools [Spec]

- In-game console.
- Live balance panel (egui).
- Performance overlay: fps, frame-time graph, entity count, memory.

### 24.5 Soak tests [Spec]

- 30 minutes at every milestone, with no crash.
- 2 hours at M7.

### 24.6 Accessibility and localization audits [Spec]

- Accessibility audit: M7.
- Localization pass: M7.
- Performance pass per preset: M7.

---

## 25. Roadmap

Status is as of 2026-11-06.

| Milestone | Scope [Spec] | Status |
|---|---|---|
| **M0 Foundation** | Workspace, pinned dependencies, M1 build settings, build script. CI for macOS and Windows. Asset pipeline, screenshot mode. Launcher skeleton that launches the game. | Done. Merged in PR #1. |
| **M1 Vertical slice** | Devourer strain. Tiers 1–2 in grey-box. Metabolism, growth, consumption feel, death, restart. Ascension only. | In review. PR #2. |
| **M2 Core systems** | Threat, mutations and Instability, ecosystem simulation. First boss. Visual pass on Tiers 1–3. Launcher v1: system scan, settings editor, shader warm-up, benchmark. | Not started. |
| **M3 Replayability** | Route map, biome variants, seeded generation, Genetic Memory. Hostility levels. Endless, Daily Seed and Speedrun modes. | Not started. |
| **M4 Land** | Tiers 4–7. Swarm and Mimic strains. The Rival. Memory fragments. Broadcasts. | Not started. |
| **M5 Cosmos** | Tiers 8–10 and all endings. Mycelial and Crystalline strains. Extinction Events, Mutator Lab, Sandbox modes. | Not started. |
| **M6 Platform** | Launcher complete: updates, integrity, mods, crash handling, Safe Mode. Mod support. Steam achievements, cloud saves, leaderboards. | Not started. |
| **M7 Polish** | Performance pass per preset. Bot-driven balance. Accessibility audit. Localization pass. 2-hour soak. | Not started. |
| **Stretch** | Versus and co-op. | Not scheduled. |

### 25.1 Dependencies [Derived]

- Genetic Memory (M3) must exist before strains can unlock (M4 and M5).
- The Rival (M4) needs local strategy statistics. Those start being recorded in M2 and M3.
- Daily Seed (M3) needs leaderboards. A local leaderboard comes in M3. Steam leaderboards come in M6.
- Endings (M5) need the Global Alert (M4) and the Rival's arc (M4).
- Localization (M7) needs a stable string set. Freeze strings at the start of M6.
- Steam (M6) needs achievements to be stable. Define achievement IDs in M3.

### 25.2 Exit criteria per milestone [Spec]

Each milestone ends with a report covering what works, what is stubbed, performance numbers and next steps. The acceptance criteria in section 26 apply at every milestone.

---

## 26. Acceptance criteria

### 26.1 Every milestone [Spec]

- A clean clone builds with `tools/build.sh mac debug`.
- CI produces a Windows build.
- All tests pass, and clippy is clean.
- Balance bots show no single death cause above 40% at Hostility 0.
- The Low preset meets its target for the content in that milestone, with numbers reported.
- Every asset is in the manifest with a verified licence.
- No hardcoded balance values.
- No crashes in a 30-minute soak. (2 hours at M7.)

### 26.2 How this document reads those criteria [Derived]

- "Hostility 0" means the base game with no Hostility modifiers. Levels 1 to 20 are not part of the acceptance run.
- "The Low preset meets its target" means 60 fps average at Low, with 1080p render scale, memory under 3.5 GB and no frame spike above 50 ms after warm-up. The target applies to the content of the milestone. It does not apply to content from later milestones.
- Numbers are recorded in `docs/STATUS.md` at each milestone.

---

## 27. Scope reality check and the "AAA" bar

### 27.1 What "AAA" means here

The specification does not use the word AAA. This section states the bar that the specification implies, so that the scope can be judged:

- **Visual fidelity** through lighting, atmosphere and motion, on every preset up to Ultra.
- **Feature completeness:** a full launcher, Steam integration, accessibility, ten or more languages and a mod system.
- **Stability:** two-hour soak with no crash.
- **Performance:** a documented target per preset, with measured numbers.
- **Content depth:** ten strata, five strains, five endings, eight modes.

The bar is high for a team of one, and that is the honest framing. The rest of this section quantifies it.

### 27.2 Content volume [Derived from the specification]

| Item | Count |
|---|---|
| Strata | 10 |
| Biome variants | 27 (10 in the first release, reading "ship 1 per tier"; 17 later) |
| Apex bosses | 10 |
| Playable strains | 5 |
| Mutation branches | 5, plus capstones and hybrid mutations (the count is not specified) |
| Event types | 7 named, each with a per-stratum table |
| Route options | 2 to 3 per transition |
| Extinction Events | 30 |
| Endings | 5 |
| Game modes | 8 (one stretch) |
| Hostility levels | 20 |
| Achievements | About 60 |
| Languages | 10 or more |
| Launcher features | 12 sections (section 20) |

### 27.3 The three largest risks [Derived]

1. **Content volume.** Ten strata with 27 biome variants, 10 bosses, five strains and a full mutation pool is a long art and design pipeline. Each new stratum adds a visual pass, a prey roster, an event table and a mutation pool.
2. **Systems balance.** The ecosystem, Threat, Instability and mutations interact. A change to one moves the others. The bots are the main defence, so they must keep up with the design.
3. **Platform work.** Launcher v1 and v2, Steam, code signing, notarization, delta updates and a signed integrity manifest are a separate body of work from the game. It has no direct effect on gameplay, and it can take as long as the gameplay.

### 27.4 Suggested cut lines [Derived, for discussion]

If the scope has to shrink, these are the lowest-cost cuts, in order:

1. Ship fewer biome variants at launch (the specification already implies one per tier).
2. Ship fewer Extinction Events at first (for example 10, then add the rest).
3. Defer Steam and launcher updates until after release. Keep integrity checks.
4. Defer Versus and Co-op. It is already a stretch goal.
5. Defer the Ultra ray-traced lighting option.

Cuts that would hurt the pillars, and should be avoided, are the Threat telegraphs, the death explanations and the size-scaled camera.

---

## 28. Where the current build departs from this vision

These are known differences between the M1 build and the specification. Most are scheduled for later milestones. The rest need a decision.

| Area | Specification | Current build | Status |
|---|---|---|---|
| Bots in CI | 10,000 runs per balance change | 400 runs | Gap. Raise when CI time allows. |
| Bot reports | Per strain and per Hostility level | One strain, no Hostility, so not split | Gap. Add the split when M3 starts. |
| Hunting rule | Every target above 1.2× hunts | Only the Hunter behaviour hunts | Decision needed, Q12 |
| Ecosystem | Lotka–Volterra-style food web | Fixed populations that respawn | M2 |
| Threat and Instability | HUD elements and mechanics | Not present | M2 |
| Events | Telegraphed, escalating | Not present | M2 |
| Mutations | 5 branches, costs, Instability, capstones | Not present | M2 |
| Genetic Memory | Points, unlocks | Death screen shows 0 | M3 |
| Strains | 5 | 1 (Devourer) | M4 and M5 |
| Audio | Adaptive score, buses, underwater filter, vibration in space | Sound effects and captions only | M2 |
| Route map | Choice of 2–3 biomes with rewards | Not present | M3 |
| Save | Run and meta saves, separated | Run saves only | M3 |
| Launcher | Full feature set | Skeleton | M2 and M6 |
| Assets | Fetched, converted, in the manifest | Procedural fallbacks only; no hashes yet | Run the fetch on the Mac |
| Performance | Measured on the M1 Mac | Not measured | Needs the Mac |
| Cold start and transitions | Under 10 s and under 3 s | Not measured | Needs the Mac |
| Accessibility | Full rebinding, motion-blur toggle, UI scaling, assist-mode game speed | Colour-blind-safe prey palette and shapes, captions, camera-shake and rumble toggles. Settings fields exist for UI scale, game speed and colour-blind mode, but their effect in play is not verified. No rebinding UI and no motion-blur toggle. | Rebinding and verification in M2. Motion-blur toggle with the visual pass (M2). |
| Localization | 10 or more languages | English only | M7 |

The M1 build also adds one thing the specification does not list: a stomach gauge in the HUD. It is kept, because it is the main way players read Gorging.

---

## 29. Open decisions

Each decision blocks the milestone named against it. They are numbered so they can be referred to.

**Identity and business**

- **Q1. Working title.** The specification's placeholder must be replaced before any store page, Steam setup or public build. Blocks: M6.
- **Q2. Business model.** Premium, free-to-play, or something else. Pricing and Steam regions. Blocks: M6.
- **Q3. Launch scope.** Which content ships in the first release: how many of the 27 biome variants, how many strains, which modes, and whether the first release covers all ten strata. Section 27.4 lists candidate cuts. Blocks: M3 (scope of the route map and the biome pool).

**Scale and progression**

- **Q4. Target run length.** The specification does not give one. M1 bots average about 294 seconds before death, and about 630 seconds to complete both tiers. Those are bot numbers, not design targets. Blocks: M2 (pacing).
- **Q5. Biomass ladder and presentation scale for Tiers 3–10.** Radius scales with the cube root of biomass, so a literal ten-stratum size range is not playable. Decide the biomass targets per tier, and how presentation scale maps onto them. Blocks: M4.
- **Q6. Transition timing.** Reconcile "under 3 seconds" with "3 to 5 seconds". The current reading is a load inside a cinematic. Blocks: M2.

**Systems**

- **Q7. Event tables.** The seven event types need a per-stratum table, telegraph durations and the Threat weight of each. Blocks: M2.
- **Q8. Global Alert.** Levels, triggers and effects. Blocks: M4.
- **Q9. Mutations.** The full mutation list, the branch themes (section 9.3, currently suggestions), the two capstone pairs that unlock hybrids, and the effect and cost of each mutation. Blocks: M2.
- **Q12. Hunting rule.** Should every target above 1.2× hunt, as the specification says, or only Hunter-behaviour species, as M1 does? Blocks: M2.
- **Q14. Genetic Memory economy.** Points per run, costs of each unlock, and how the economy keeps unlocks from becoming power. Blocks: M3.
- **Q15. Hostility modifiers.** What each of the 20 levels changes, and its reward. Blocks: M3.
- **Q16. The Rival.** The adaptation model, the encounter schedule and the arc beats. Blocks: M4.
- **Q17. Ending triggers.** Thresholds for Symbiosis, the Seed trigger, Containment, the Secret trigger and the Consume-the-Universe condition. Blocks: M5.
- **Q18. Endless Bloom scoring.** What counts as a score and how Threat escalates over time. Blocks: M3.

**Content**

- **Q10. Strain mechanics.** Swarm partial losses, Mycelial spread and interaction with Threat, Mimic assimilation limits, Crystalline energy sources. Blocks: M4 and M5.
- **Q11. Biome variants versus route options.** "Ship 1 per tier" conflicts with a choice of 2–3 biomes per transition. Decide what the route offers at launch. Blocks: M3.
- **Q13. Boss mechanics.** For all ten apex bosses. The first boss is in M2. Blocks: M2.
- **Q19. Story bible.** The memory fragments, the broadcasts and the Rival's history need a written bible before M4. Blocks: M4.

**Technical**

- **Q20. Determinism across platforms.** The Windows and macOS builds must produce identical runs for a given seed, or the shared-seed features break. Blocks: M3.
- **Q21. Generator versioning.** How seeds carry the generator version, and what happens to an old seed after a generator change. Blocks: M3.
- **Q22. Steam scope.** Which features ship in the first Steam build, and which later. Blocks: M6.
- **Q23. Netcode authority model.** Only if co-op is built. Blocks: stretch.

---

## 30. Glossary

| Term | Meaning |
|---|---|
| **Apex boss** | The strongest creature of a stratum. Defeating it grants a mutation choice. |
| **Ascension** | The full run, Tiers 1 to 10. |
| **Biomass (B)** | Body mass. Sets the organism's radius and its growth stage. |
| **Capstone** | The ability unlocked by four mutations from one branch. |
| **Daily Seed** | A fixed seed with fixed mutators, one attempt per day. |
| **Dangerous** | Prey more than 1.2× the organism's size. It cannot be eaten. |
| **Edible** | Prey at or below 0.8× the organism's size. |
| **Energy (E)** | Fuel, 0 to 100. Drained by upkeep. |
| **Extinction Event** | A hand-designed challenge scenario. |
| **Genetic Instability** | The cost of mixing mutation branches. |
| **Genetic Memory** | Meta-progression currency that unlocks options, never raw power. |
| **Global Alert** | From Tier 5, humanity's escalating response across strata. |
| **Gorged** | Stomach above 150% of capacity. Slower, and takes extra damage. |
| **Hit-stop** | A brief pause of virtual time on a major event. |
| **Hostility** | Stackable difficulty modifiers, levels 1 to 20. |
| **Integrity** | Health. Death at 0. |
| **Instability** | The player's mutation-mixing risk, 0 to 100. |
| **Kleiber's law** | Metabolic rate scales with biomass to the 0.75 power. |
| **Mutation** | A permanent change chosen at a transition or a boss kill. |
| **Risky** | Prey between 0.8× and 1.2× the organism's size. Triggers a struggle. |
| **Rival** | An older organism from a previous meteor, which recurs across runs. |
| **Stratum (Tier)** | A region of the run. Ten in total. |
| **Strain** | A playable starting organism, with its own core verb. |
| **Struggle** | A skill-based contest with risky prey. |
| **Threat** | Per-stratum pressure, 0 to 100, which fires escalating events. |
| **Route map** | The choice of next biome after each tier. |
| **Tier-up** | The transition into the next stratum, with its cinematic. |
