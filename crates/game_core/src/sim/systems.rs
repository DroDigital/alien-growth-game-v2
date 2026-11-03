//! Fixed-step simulation systems. See the module docs in `sim/mod.rs`.

use super::*;
use crate::balance::Behavior;
use crate::rules::{SizeClass, StruggleOutcome, bite_damage, classify, speed_for};

/// Exponential approach factor for a rate (1/s) over `dt`.
fn approach(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

pub(super) fn record_prev_pos(mut q: Query<(&Pos, &mut PrevPos)>) {
    for (p, mut prev) in &mut q {
        prev.0 = p.0;
    }
}

pub(super) fn agent_ai(
    time: Res<Time>,
    balance: Res<Balance>,
    info: Res<RunInfo>,
    mut rng: ResMut<SimRng>,
    player: Query<(&Pos, &Radius), With<Player>>,
    mut agents: Query<(&mut Agent, &Pos, &mut Vel, &Radius), Without<Grappled>>,
) {
    let dt = time.delta_secs();
    let Ok((ppos, pr)) = player.single() else {
        return;
    };
    let tier = info.tier(&balance);
    let gate = &balance.core.size_gate;
    let loco = &balance.core.locomotion;
    for (mut agent, pos, mut vel, r) in &mut agents {
        let species = &tier.species[agent.species as usize];
        agent.bite_cooldown = (agent.bite_cooldown - dt).max(0.0);
        let speed = speed_for(r.0, species.speed, loco);
        let to_player = ppos.0 - pos.0;
        let dist = to_player.length();
        let sensed = dist < species.sense * r.0 + pr.0;
        // From the agent's point of view: could the player eat me, or could I eat the player?
        let player_threat = classify(r.0, pr.0, gate) != SizeClass::Dangerous;
        let combat = &balance.core.combat;
        let player_is_prey =
            r.0 > gate.struggle_max_ratio * pr.0 && pr.0 >= combat.hunt_min_ratio * r.0;
        // A tired hunter rests until it has recovered most of its stamina.
        let can_hunt = agent.stamina
            > if agent.mode == AgentMode::Hunt {
                0.0
            } else {
                0.6
            };

        let mode = match species.behavior {
            Behavior::Hunter if sensed && player_is_prey && can_hunt => AgentMode::Hunt,
            Behavior::Hunter | Behavior::Skittish if sensed && player_threat => AgentMode::Flee,
            _ => AgentMode::Wander,
        };
        agent.mode = mode;

        let desired = match mode {
            AgentMode::Hunt => {
                agent.stamina = (agent.stamina - combat.hunt_stamina_per_sec * dt).max(0.0);
                to_player.normalize_or_zero() * speed
            }
            AgentMode::Flee => {
                agent.stamina = (agent.stamina - 0.3 * dt).max(0.0);
                let boost = if agent.stamina > 0.0 { 1.0 } else { 0.45 };
                (-to_player).normalize_or_zero() * speed * boost
            }
            AgentMode::Wander => {
                agent.stamina = (agent.stamina + 0.15 * dt).min(1.0);
                agent.wander_timer -= dt;
                if agent.wander_timer <= 0.0 {
                    agent.wander_dir = random_dir(&mut rng.0);
                    agent.wander_timer = rng.0.random_range(1.5..5.0);
                }
                let pace = match species.behavior {
                    Behavior::Drifter | Behavior::Grazer => 0.6,
                    _ => 0.45,
                };
                agent.wander_dir * speed * pace
            }
        };
        vel.0 = vel.0.lerp(desired, approach(3.0, dt));
    }
}

pub(super) fn player_motion(
    time: Res<Time>,
    balance: Res<Balance>,
    info: Res<RunInfo>,
    intent: Res<PlayerIntent>,
    mut q: Query<(
        &mut Player,
        &mut PlayerVitals,
        &mut Vel,
        &Radius,
        Option<&Struggling>,
    )>,
) {
    let dt = time.delta_secs();
    let Some(strain) = balance.strain(&info.strain) else {
        return;
    };
    let met = &balance.core.metabolism;
    for (mut player, mut vitals, mut vel, r, struggling) in &mut q {
        player.dash_cooldown = (player.dash_cooldown - dt).max(0.0);
        player.dash_timer = (player.dash_timer - dt).max(0.0);
        player.hurt_timer = (player.hurt_timer - dt).max(0.0);
        if struggling.is_some() {
            vel.0 = vel.0.lerp(Vec3::ZERO, approach(10.0, dt));
            continue;
        }
        if intent.dash
            && player.dash_cooldown <= 0.0
            && vitals.0.spend_energy(strain.dash_energy_cost)
        {
            player.dash_timer = strain.dash_secs;
            player.dash_cooldown = strain.dash_cooldown_secs;
        }
        let mut speed = speed_for(r.0, strain.speed_factor, &balance.core.locomotion);
        if player.dash_timer > 0.0 {
            speed *= strain.dash_speed_multiplier;
        }
        if vitals.0.is_gorged(met) {
            speed *= met.gorged_speed_multiplier;
        }
        let desired = intent.move_dir.clamp_length_max(1.0) * speed;
        let accel = if player.dash_timer > 0.0 {
            strain.acceleration * 3.0
        } else {
            strain.acceleration
        };
        vel.0 = vel.0.lerp(desired, approach(accel, dt));
    }
}

pub(super) fn integrate(
    time: Res<Time>,
    arena: Res<Arena>,
    mut q: Query<(&mut Pos, &mut Vel, &Radius, Option<&Agent>)>,
) {
    let dt = time.delta_secs();
    for (mut pos, mut vel, r, agent) in &mut q {
        pos.0 += vel.0 * dt;
        // Horizontal arena bound.
        let flat = Vec2::new(pos.0.x, pos.0.z);
        let max = (arena.radius - r.0).max(0.0);
        if flat.length() > max {
            let n = flat.normalize();
            let clamped = n * max;
            pos.0.x = clamped.x;
            pos.0.z = clamped.y;
            let outward = vel.0.x * n.x + vel.0.z * n.y;
            if outward > 0.0 {
                vel.0.x -= outward * n.x;
                vel.0.z -= outward * n.y;
            }
            if agent.is_some() {
                // Agents turn back toward the middle.
                vel.0.x -= n.x * 0.5;
                vel.0.z -= n.y * 0.5;
            }
        }
        // Floor and ceiling.
        let floor = arena.floor(pos.0.x, pos.0.z) + r.0;
        if pos.0.y < floor {
            pos.0.y = floor;
            vel.0.y = vel.0.y.max(0.0);
        }
        let ceiling = arena.ceiling - r.0;
        if pos.0.y > ceiling {
            pos.0.y = ceiling.max(floor);
            vel.0.y = vel.0.y.min(0.0);
        }
    }
}

/// Records a death once and moves the run to the death screen.
fn kill(
    info: &mut RunInfo,
    log: &mut RunLog,
    died: &mut MessageWriter<PlayerDied>,
    next: &mut NextState<RunState>,
    cause: DeathCause,
) {
    if info.outcome.is_some() {
        return;
    }
    log.push(info.elapsed, LogKind::Died(cause.clone()));
    info.outcome = Some(RunOutcome::Died(cause.clone()));
    died.write(PlayerDied { cause });
    next.set(RunState::Dead);
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Outcomes<'w> {
    info: ResMut<'w, RunInfo>,
    log: ResMut<'w, RunLog>,
    died: MessageWriter<'w, PlayerDied>,
    next: ResMut<'w, NextState<RunState>>,
    pending: ResMut<'w, PendingRespawns>,
    consumed: MessageWriter<'w, Consumed>,
}

impl Outcomes<'_> {
    fn kill(&mut self, cause: DeathCause) {
        kill(
            &mut self.info,
            &mut self.log,
            &mut self.died,
            &mut self.next,
            cause,
        );
    }

    fn eat(
        &mut self,
        balance: &Balance,
        vitals: &mut Vitals,
        species: u16,
        prey_pos: Vec3,
        prey_radius: f32,
        player_radius: f32,
    ) {
        let mass = prey_mass(prey_radius, balance);
        vitals.ingest(mass, &balance.core.metabolism);
        self.info.meals += 1;
        let tier = self.info.tier(balance);
        let id = tier.species[species as usize].id.clone();
        if self.info.meals <= 3 || self.info.meals.is_multiple_of(MEAL_LOG_EVERY) {
            let t = self.info.elapsed;
            self.log.push(t, LogKind::Meal { species: id, mass });
        }
        self.pending.0.push((species, tier.respawn_secs));
        self.consumed.write(Consumed {
            species,
            pos: prey_pos,
            radius: prey_radius,
            mass,
            ratio: prey_radius / player_radius,
        });
    }
}

pub(super) fn contacts(
    mut commands: Commands,
    balance: Res<Balance>,
    mut out: Outcomes,
    mut player: Query<(
        Entity,
        &mut Player,
        &mut PlayerVitals,
        &Pos,
        &mut Vel,
        &Radius,
        Has<Struggling>,
    )>,
    mut agents: Query<
        (Entity, &mut Agent, &Pos, &mut Vel, &Radius),
        (Without<Player>, Without<Grappled>),
    >,
    mut started: MessageWriter<StruggleStarted>,
    mut bitten: MessageWriter<PlayerBitten>,
) {
    if out.info.outcome.is_some() {
        return;
    }
    let Ok((pe, mut pl, mut vitals, ppos, mut pvel, pr, mut struggling)) = player.single_mut()
    else {
        return;
    };
    let Some(strain) = balance.strain(&out.info.strain).cloned() else {
        return;
    };
    let tier = out.info.tier(&balance).clone();
    let gate = &balance.core.size_gate;
    let met = &balance.core.metabolism;
    let combat = &balance.core.combat;
    for (ae, mut agent, apos, mut avel, ar) in &mut agents {
        let d = ppos.0.distance(apos.0);
        let class = classify(ar.0, pr.0, gate);
        match class {
            SizeClass::Edible => {
                if !struggling && d < (pr.0 + ar.0) * strain.bite_reach && vitals.0.can_ingest(met)
                {
                    commands.entity(ae).despawn();
                    out.eat(&balance, &mut vitals.0, agent.species, apos.0, ar.0, pr.0);
                }
            }
            SizeClass::Risky => {
                if !struggling && d < (pr.0 + ar.0) * strain.bite_reach && vitals.0.can_ingest(met)
                {
                    commands.entity(ae).insert(Grappled);
                    commands.entity(pe).insert(Struggling {
                        target: ae,
                        species: agent.species,
                        struggle: Struggle::new(ar.0 / pr.0, &balance.core.struggle),
                    });
                    avel.0 = Vec3::ZERO;
                    started.write(StruggleStarted {
                        species: agent.species,
                        target: ae,
                    });
                    // One struggle at a time.
                    struggling = true;
                }
            }
            SizeClass::Dangerous => {
                if d >= pr.0 + ar.0 {
                    continue;
                }
                let away = (ppos.0 - apos.0).normalize_or(Vec3::Y);
                let species = &tier.species[agent.species as usize];
                if species.behavior == Behavior::Hunter
                    && agent.bite_cooldown <= 0.0
                    && pl.hurt_timer <= 0.0
                {
                    let damage =
                        bite_damage(ar.0 / pr.0, combat.bite_damage, combat.max_damage_ratio);
                    agent.bite_cooldown = combat.bite_cooldown_secs;
                    pl.hurt_timer = 0.4;
                    pvel.0 += away * combat.knockback * pr.0;
                    bitten.write(PlayerBitten {
                        species: agent.species,
                        damage,
                        from: apos.0,
                    });
                    let t = out.info.elapsed;
                    out.log.push(
                        t,
                        LogKind::Bitten {
                            species: species.id.clone(),
                            damage,
                        },
                    );
                    if vitals.0.damage(damage, met) {
                        out.kill(DeathCause::Predation(species.id.clone()));
                        return;
                    }
                } else {
                    // Bump: push apart without damage.
                    pvel.0 += away * 2.0 * pr.0;
                }
            }
        }
    }
}

pub(super) fn struggle_step(
    mut commands: Commands,
    time: Res<Time>,
    balance: Res<Balance>,
    intent: Res<PlayerIntent>,
    mut out: Outcomes,
    mut player: Query<(Entity, &mut PlayerVitals, &Pos, &Radius, &mut Struggling), Without<Agent>>,
    mut agents: Query<(&mut Agent, &mut Pos, &mut Vel, &Radius), With<Grappled>>,
    mut ended: MessageWriter<StruggleEnded>,
) {
    let dt = time.delta_secs();
    let Ok((pe, mut vitals, ppos, pr, mut s)) = player.single_mut() else {
        return;
    };
    if out.info.outcome.is_some() {
        return;
    }
    let species = s.species;
    let Ok((mut agent, mut apos, mut avel, ar)) = agents.get_mut(s.target) else {
        // Target vanished (tier change); just end the struggle.
        commands.entity(pe).remove::<Struggling>();
        return;
    };
    // Hold the target against the body.
    let dir = (apos.0 - ppos.0).normalize_or(Vec3::X);
    apos.0 = ppos.0 + dir * (pr.0 + ar.0) * 0.85;
    avel.0 = Vec3::ZERO;

    let outcome = s.struggle.step(
        dt,
        intent.consume_presses,
        &balance.core.struggle,
        &balance.core.size_gate,
    );
    let id = out.info.tier(&balance).species[species as usize].id.clone();
    match outcome {
        StruggleOutcome::Ongoing => {}
        StruggleOutcome::Won => {
            commands.entity(pe).remove::<Struggling>();
            commands.entity(s.target).despawn();
            let t = out.info.elapsed;
            out.log.push(t, LogKind::StruggleWon(id));
            out.eat(&balance, &mut vitals.0, species, apos.0, ar.0, pr.0);
            ended.write(StruggleEnded { species, won: true });
        }
        StruggleOutcome::Lost => {
            commands.entity(pe).remove::<Struggling>();
            commands.entity(s.target).remove::<Grappled>();
            agent.mode = AgentMode::Flee;
            agent.stamina = 1.0;
            avel.0 = dir * 3.0 * ar.0;
            let t = out.info.elapsed;
            out.log.push(t, LogKind::StruggleLost(id.clone()));
            ended.write(StruggleEnded {
                species,
                won: false,
            });
            let met = &balance.core.metabolism;
            if vitals.0.damage(balance.core.struggle.fail_damage, met) {
                out.kill(DeathCause::StruggleInjury(id));
            }
        }
    }
}

pub(super) fn metabolism(
    time: Res<Time>,
    balance: Res<Balance>,
    mut out: Outcomes,
    mut q: Query<(&mut Player, &mut PlayerVitals, &mut Radius)>,
) {
    if out.info.outcome.is_some() {
        return;
    }
    let dt = time.delta_secs();
    let met = &balance.core.metabolism;
    let Some(strain) = balance.strain(&out.info.strain) else {
        return;
    };
    let k = out.info.tier(&balance).upkeep_k * strain.upkeep_multiplier;
    for (mut player, mut vitals, mut radius) in &mut q {
        let report = vitals.0.tick(dt, met, k);
        radius.0 = vitals.0.radius(met);
        out.info.peak_biomass = out.info.peak_biomass.max(vitals.0.biomass);
        let t = out.info.elapsed;
        if report.starving && !player.was_starving {
            out.log.push(t, LogKind::Starving);
        }
        if report.gorged && !player.was_gorged {
            out.log.push(t, LogKind::Gorged);
        }
        player.was_starving = report.starving;
        player.was_gorged = report.gorged;
        if report.died {
            out.kill(DeathCause::Starvation);
        }
    }
}

pub(super) fn progress(
    time: Res<Time>,
    balance: Res<Balance>,
    mut info: ResMut<RunInfo>,
    mut log: ResMut<RunLog>,
    mut next: ResMut<NextState<RunState>>,
    mut completed: MessageWriter<TierCompleted>,
    player: Query<(&Pos, &PlayerVitals), With<Player>>,
) {
    let dt = time.delta_secs();
    if info.outcome.is_some() {
        return;
    }
    let before = info.elapsed;
    info.elapsed += dt;
    info.tier_elapsed += dt;
    let Ok((pos, vitals)) = player.single() else {
        return;
    };
    if (before / SAMPLE_SECS).floor() != (info.elapsed / SAMPLE_SECS).floor() {
        let (t, tier) = (info.elapsed, info.tier_index);
        log.samples.push((t, tier, pos.0));
    }
    let tier = info.tier(&balance);
    if vitals.0.biomass >= tier.complete_biomass {
        let last = info.tier_index >= balance.last_tier_index();
        let tier_no = tier.tier;
        log.push(info.elapsed, LogKind::TierCompleted(tier_no));
        completed.write(TierCompleted {
            tier: tier_no,
            last,
        });
        if last {
            info.outcome = Some(RunOutcome::Completed);
            next.set(RunState::Victory);
        } else {
            next.set(RunState::TierTransition);
        }
    }
}

pub(super) fn respawn(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let mut due: Vec<u16> = Vec::new();
    {
        let mut pending = world.resource_mut::<PendingRespawns>();
        for (_, t) in pending.0.iter_mut() {
            *t -= dt;
        }
        pending.0.retain(|(s, t)| {
            if *t <= 0.0 {
                due.push(*s);
                false
            } else {
                true
            }
        });
    }
    if due.is_empty() {
        return;
    }
    let balance = world.resource::<Balance>().clone();
    let tier_index = world.resource::<RunInfo>().tier_index;
    let tier = balance.tier(tier_index).expect("tier").clone();
    let arena = world.resource::<Arena>().clone();
    let (ppos, pr) = world
        .query_filtered::<(&Pos, &Radius), With<Player>>()
        .single(world)
        .map(|(p, r)| (p.0, r.0))
        .unwrap_or((Vec3::ZERO, 1.0));
    let mut rng = std::mem::replace(
        &mut world.resource_mut::<SimRng>().0,
        ChaCha8Rng::seed_from_u64(0),
    );
    for s in due {
        spawn_agent(
            world,
            &mut rng,
            &arena,
            s,
            &tier.species[s as usize],
            ppos,
            pr,
        );
    }
    world.resource_mut::<SimRng>().0 = rng;
}

pub(super) fn clear_intent(mut intent: ResMut<PlayerIntent>) {
    intent.consume_presses = 0;
    intent.dash = false;
}
