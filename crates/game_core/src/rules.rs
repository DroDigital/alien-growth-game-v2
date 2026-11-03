//! Pure game rules: metabolism, growth, the size gate and struggles.
//! No ECS, no rendering; every function here is unit-tested.

use crate::balance::{LocomotionParams, MetabolismParams, SizeGateParams, StruggleParams};
use serde::{Deserialize, Serialize};

/// Body radius for a biomass (radius grows with the cube root of mass).
pub fn radius_for_biomass(biomass: f32, base_radius: f32) -> f32 {
    base_radius * biomass.max(0.0).cbrt()
}

/// Mass of a creature of the given radius (same density as the player).
pub fn mass_for_radius(radius: f32, base_radius: f32) -> f32 {
    (radius / base_radius).powi(3)
}

/// Cruise speed for a body of `radius` with a species/strain factor.
pub fn speed_for(radius: f32, factor: f32, loco: &LocomotionParams) -> f32 {
    loco.base_speed * factor * radius.max(1e-4).powf(loco.speed_exponent)
}

/// How a target compares to the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SizeClass {
    /// At most `safe_ratio` of the player's size: eaten on contact.
    Edible,
    /// Between the ratios: a skill-based struggle decides.
    Risky,
    /// Larger than `struggle_max_ratio`: cannot be eaten.
    Dangerous,
}

pub fn classify(target_radius: f32, player_radius: f32, gate: &SizeGateParams) -> SizeClass {
    let ratio = target_radius / player_radius.max(1e-6);
    if ratio <= gate.safe_ratio {
        SizeClass::Edible
    } else if ratio <= gate.struggle_max_ratio {
        SizeClass::Risky
    } else {
        SizeClass::Dangerous
    }
}

/// Why a run ended.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeathCause {
    /// Energy ran out and integrity drained away.
    Starvation,
    /// Bitten to death by a larger creature (species id).
    Predation(String),
    /// Injured by prey that broke free from a struggle (species id).
    StruggleInjury(String),
}

impl DeathCause {
    /// Stable key for statistics and localization (`death-<key>`).
    pub fn key(&self) -> String {
        match self {
            DeathCause::Starvation => "starvation".into(),
            DeathCause::Predation(s) => format!("predation:{s}"),
            DeathCause::StruggleInjury(s) => format!("struggle:{s}"),
        }
    }
}

/// The player's vital statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    pub biomass: f32,
    /// Energy, % of capacity (0..=100).
    pub energy: f32,
    /// Integrity (health), % (0..=100).
    pub integrity: f32,
    /// Undigested mass waiting in the digestion queue.
    pub stomach: f32,
}

/// What happened during one metabolism tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TickReport {
    pub digested: f32,
    pub biomass_gained: f32,
    pub starving: bool,
    pub gorged: bool,
    pub died: bool,
}

impl Vitals {
    pub fn new(biomass: f32, met: &MetabolismParams) -> Self {
        Self {
            biomass,
            energy: met.start_energy,
            integrity: 100.0,
            stomach: 0.0,
        }
    }

    pub fn radius(&self, met: &MetabolismParams) -> f32 {
        radius_for_biomass(self.biomass, met.base_radius)
    }

    pub fn stomach_capacity(&self, met: &MetabolismParams) -> f32 {
        met.stomach_capacity_per_biomass * self.biomass
    }

    /// Stomach fill as a fraction of capacity (1.0 = full, 1.5 = gorged).
    pub fn stomach_fill(&self, met: &MetabolismParams) -> f32 {
        self.stomach / self.stomach_capacity(met).max(1e-6)
    }

    pub fn is_gorged(&self, met: &MetabolismParams) -> bool {
        self.stomach_fill(met) > met.gorged_threshold
    }

    pub fn can_ingest(&self, met: &MetabolismParams) -> bool {
        self.stomach_fill(met) < met.max_stomach_fill
    }

    /// Adds eaten mass to the digestion queue. Returns false if nothing fits.
    pub fn ingest(&mut self, mass: f32, met: &MetabolismParams) -> bool {
        if !self.can_ingest(met) {
            return false;
        }
        self.stomach += mass.max(0.0);
        true
    }

    /// Upkeep in % of energy capacity per second (Kleiber: k × B^0.75).
    pub fn upkeep_percent_per_sec(&self, met: &MetabolismParams, k: f32) -> f32 {
        let absolute = k * self.biomass.powf(met.kleiber_exponent);
        absolute / (met.energy_capacity_per_biomass * self.biomass) * 100.0
    }

    /// Spends energy (%) for an action; returns false if there was not enough.
    pub fn spend_energy(&mut self, percent: f32) -> bool {
        if self.energy < percent {
            return false;
        }
        self.energy -= percent;
        true
    }

    /// Applies integrity damage, amplified while gorged. Returns true if this killed.
    pub fn damage(&mut self, amount: f32, met: &MetabolismParams) -> bool {
        let mult = if self.is_gorged(met) {
            met.gorged_damage_multiplier
        } else {
            1.0
        };
        self.integrity = (self.integrity - amount * mult).max(0.0);
        self.integrity <= 0.0
    }

    /// Advances digestion, upkeep, starvation and regeneration by `dt` seconds.
    /// `k` is the stratum upkeep constant times the strain multiplier.
    pub fn tick(&mut self, dt: f32, met: &MetabolismParams, k: f32) -> TickReport {
        let mut report = TickReport {
            gorged: self.is_gorged(met),
            ..Default::default()
        };

        // Digestion: queue -> biomass + energy.
        let rate = met.digestion_rate * self.biomass.powf(met.kleiber_exponent);
        let digested = (rate * dt).min(self.stomach);
        if digested > 0.0 {
            self.stomach -= digested;
            let capacity = met.energy_capacity_per_biomass * self.biomass;
            self.energy += met.energy_yield_per_mass * digested / capacity * 100.0;
            let gained = digested * met.growth_fraction;
            self.biomass += gained;
            report.digested = digested;
            report.biomass_gained = gained;
        }

        // Upkeep.
        self.energy -= self.upkeep_percent_per_sec(met, k) * dt;
        if self.energy <= 0.0 {
            self.energy = 0.0;
            report.starving = true;
            self.integrity = (self.integrity - met.starvation_damage_per_sec * dt).max(0.0);
        } else if self.energy >= met.regen_min_energy {
            self.integrity = (self.integrity + met.integrity_regen_per_sec * dt).min(100.0);
        }
        self.energy = self.energy.min(100.0);
        report.died = self.integrity <= 0.0;
        report
    }
}

/// A struggle against a target between `safe_ratio` and `struggle_max_ratio`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Struggle {
    /// 0 = target escapes, 1 = target consumed.
    pub progress: f32,
    pub elapsed: f32,
    /// Target size / player size when the struggle began.
    pub ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StruggleOutcome {
    Ongoing,
    Won,
    Lost,
}

impl Struggle {
    pub fn new(ratio: f32, params: &StruggleParams) -> Self {
        Self {
            progress: params.start_progress,
            elapsed: 0.0,
            ratio,
        }
    }

    /// Resistance (progress lost per second) for this struggle's size ratio.
    pub fn resistance(&self, params: &StruggleParams, gate: &SizeGateParams) -> f32 {
        params.resist_per_sec * (self.ratio / gate.safe_ratio).powf(params.resist_ratio_power)
    }

    /// Advances by `dt` with `presses` Consume presses this step.
    pub fn step(
        &mut self,
        dt: f32,
        presses: u32,
        params: &StruggleParams,
        gate: &SizeGateParams,
    ) -> StruggleOutcome {
        self.elapsed += dt;
        self.progress += presses as f32 * params.press_gain;
        self.progress -= self.resistance(params, gate) * dt;
        if self.progress >= 1.0 {
            StruggleOutcome::Won
        } else if self.progress <= 0.0 || self.elapsed >= params.timeout_secs {
            StruggleOutcome::Lost
        } else {
            StruggleOutcome::Ongoing
        }
    }
}

/// Integrity damage of one bite from a creature `ratio` times the player's size.
pub fn bite_damage(ratio: f32, base: f32, max_ratio: f32) -> f32 {
    base * ratio.clamp(1.0, max_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::balance::Balance;

    fn bal() -> Balance {
        Balance::builtin()
    }

    #[test]
    fn radius_and_mass_are_inverse() {
        let r = radius_for_biomass(27.0, 0.25);
        assert!((r - 0.75).abs() < 1e-5);
        assert!((mass_for_radius(r, 0.25) - 27.0).abs() < 1e-3);
    }

    #[test]
    fn size_gate_boundaries() {
        let g = bal().core.size_gate;
        assert_eq!(classify(0.8, 1.0, &g), SizeClass::Edible);
        assert_eq!(classify(0.81, 1.0, &g), SizeClass::Risky);
        assert_eq!(classify(1.2, 1.0, &g), SizeClass::Risky);
        assert_eq!(classify(1.21, 1.0, &g), SizeClass::Dangerous);
    }

    #[test]
    fn upkeep_follows_kleiber() {
        let met = bal().core.metabolism;
        let small = Vitals::new(1.0, &met);
        let big = Vitals::new(1000.0, &met);
        // Absolute upkeep scales with B^0.75, so relative upkeep falls as B^-0.25.
        let ratio = small.upkeep_percent_per_sec(&met, 1.0) / big.upkeep_percent_per_sec(&met, 1.0);
        assert!((ratio - 1000f32.powf(0.25)).abs() < 1e-2);
    }

    #[test]
    fn starvation_drains_integrity_then_kills() {
        let met = bal().core.metabolism;
        let mut v = Vitals::new(1.0, &met);
        v.energy = 0.0;
        let mut t = 0.0;
        while !v.tick(0.1, &met, 0.3).died {
            t += 0.1;
            assert!(t < 60.0, "should starve within a minute");
        }
        assert!((t - 100.0 / met.starvation_damage_per_sec).abs() < 0.5);
    }

    #[test]
    fn digestion_grows_biomass_and_restores_energy() {
        let met = bal().core.metabolism;
        let mut v = Vitals::new(1.0, &met);
        v.energy = 20.0;
        assert!(v.ingest(0.4, &met));
        let mut gained = 0.0;
        for _ in 0..600 {
            gained += v.tick(0.1, &met, 0.0).biomass_gained;
        }
        assert!(v.stomach < 1e-4, "queue empties");
        assert!((gained - 0.4 * met.growth_fraction).abs() < 1e-3);
        assert!(v.energy > 20.0);
    }

    #[test]
    fn gorging_blocks_eating_and_amplifies_damage() {
        let met = bal().core.metabolism;
        let mut v = Vitals::new(10.0, &met);
        let cap = v.stomach_capacity(&met);
        assert!(v.ingest(cap * 1.6, &met));
        assert!(v.is_gorged(&met));
        assert!(v.ingest(cap * 0.5, &met), "still room below max fill");
        assert!(!v.ingest(1.0, &met), "full above max_stomach_fill");
        v.damage(10.0, &met);
        assert!((v.integrity - (100.0 - 10.0 * met.gorged_damage_multiplier)).abs() < 1e-4);
    }

    #[test]
    fn struggle_needs_presses_and_harder_when_bigger() {
        let b = bal();
        let (p, g) = (&b.core.struggle, &b.core.size_gate);
        let mut idle = Struggle::new(1.0, p);
        let mut outcome = StruggleOutcome::Ongoing;
        while outcome == StruggleOutcome::Ongoing {
            outcome = idle.step(1.0 / 64.0, 0, p, g);
        }
        assert_eq!(outcome, StruggleOutcome::Lost);

        // Mashing at 8 presses/s wins against a small risky target.
        let mut s = Struggle::new(0.85, p);
        let mut t = 0;
        let outcome = loop {
            let presses = u32::from(t % 8 == 0);
            let o = s.step(1.0 / 64.0, presses, p, g);
            t += 1;
            if o != StruggleOutcome::Ongoing {
                break o;
            }
        };
        assert_eq!(outcome, StruggleOutcome::Won);
        let small = Struggle::new(0.85, p).resistance(p, g);
        let big = Struggle::new(1.2, p).resistance(p, g);
        assert!(big > small * 2.0);
    }

    #[test]
    fn bite_damage_scales_and_caps() {
        assert_eq!(bite_damage(0.5, 10.0, 2.5), 10.0);
        assert_eq!(bite_damage(2.0, 10.0, 2.5), 20.0);
        assert_eq!(bite_damage(9.0, 10.0, 2.5), 25.0);
    }

    #[test]
    fn death_cause_keys_are_stable() {
        assert_eq!(DeathCause::Starvation.key(), "starvation");
        assert_eq!(
            DeathCause::Predation("moray".into()).key(),
            "predation:moray"
        );
    }
}
