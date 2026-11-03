//! Balance data: every tunable game number, loaded from `data/balance/*.ron`.
//!
//! The same files are compiled into the binary as a fallback
//! ([`Balance::builtin`]) so tests and bots never depend on the working
//! directory, and a broken file never stops the game from starting.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetabolismParams {
    pub kleiber_exponent: f32,
    pub energy_capacity_per_biomass: f32,
    pub start_energy: f32,
    pub digestion_rate: f32,
    pub energy_yield_per_mass: f32,
    pub growth_fraction: f32,
    pub stomach_capacity_per_biomass: f32,
    pub gorged_threshold: f32,
    pub max_stomach_fill: f32,
    pub gorged_speed_multiplier: f32,
    pub gorged_damage_multiplier: f32,
    pub starvation_damage_per_sec: f32,
    pub integrity_regen_per_sec: f32,
    pub regen_min_energy: f32,
    pub base_radius: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SizeGateParams {
    pub safe_ratio: f32,
    pub struggle_max_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StruggleParams {
    pub start_progress: f32,
    pub press_gain: f32,
    pub resist_per_sec: f32,
    pub resist_ratio_power: f32,
    pub fail_damage: f32,
    pub timeout_secs: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatParams {
    pub bite_damage: f32,
    pub max_damage_ratio: f32,
    pub bite_cooldown_secs: f32,
    pub knockback: f32,
    pub hunt_min_ratio: f32,
    pub hunt_stamina_per_sec: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocomotionParams {
    pub base_speed: f32,
    pub speed_exponent: f32,
}

/// `data/balance/metabolism.ron`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreBalance {
    pub metabolism: MetabolismParams,
    pub size_gate: SizeGateParams,
    pub struggle: StruggleParams,
    pub combat: CombatParams,
    pub locomotion: LocomotionParams,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrainParams {
    pub name_key: String,
    pub upkeep_multiplier: f32,
    pub speed_factor: f32,
    pub acceleration: f32,
    pub dash_speed_multiplier: f32,
    pub dash_secs: f32,
    pub dash_cooldown_secs: f32,
    pub dash_energy_cost: f32,
    pub bite_reach: f32,
}

/// `data/balance/strains.ron`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrainTable {
    pub strains: BTreeMap<String, StrainParams>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Biome {
    CraterSeafloor,
    CoralReef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Behavior {
    /// Wanders slowly; never flees or attacks.
    Drifter,
    /// Wanders; flees anything that could eat it.
    Skittish,
    /// Hunts the player while the player is small enough to be dangerous to.
    Hunter,
    /// Wanders, ignores everything.
    Grazer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Shape {
    Blob,
    Elongated,
    Jelly,
    Spiky,
    Fish,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesDef {
    pub id: String,
    pub name_key: String,
    pub radius: (f32, f32),
    pub count: u32,
    pub behavior: Behavior,
    pub speed: f32,
    pub sense: f32,
    pub shape: Shape,
    pub color: (f32, f32, f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierDef {
    pub tier: u8,
    pub stratum_key: String,
    pub biome_key: String,
    pub biome: Biome,
    pub upkeep_k: f32,
    pub start_biomass: f32,
    pub complete_biomass: f32,
    pub arena_radius: f32,
    pub ceiling: f32,
    pub respawn_secs: f32,
    pub species: Vec<SpeciesDef>,
}

/// `data/balance/strata.ron`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrataTable {
    pub tiers: Vec<TierDef>,
}

/// Everything the simulation needs, as one value.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
pub struct Balance {
    pub core: CoreBalance,
    pub strains: StrainTable,
    pub strata: StrataTable,
}

#[derive(Debug, thiserror::Error)]
pub enum BalanceError {
    #[error("I/O error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Ron {
        path: String,
        source: Box<ron::error::SpannedError>,
    },
    #[error("invalid balance data: {0}")]
    Invalid(String),
}

const BUILTIN_CORE: &str = include_str!("../../../data/balance/metabolism.ron");
const BUILTIN_STRAINS: &str = include_str!("../../../data/balance/strains.ron");
const BUILTIN_STRATA: &str = include_str!("../../../data/balance/strata.ron");

fn parse<T: serde::de::DeserializeOwned>(src: &str, path: &str) -> Result<T, BalanceError> {
    ron::from_str(src).map_err(|source| BalanceError::Ron {
        path: path.to_string(),
        source: Box::new(source),
    })
}

impl Balance {
    /// The balance compiled into the binary.
    pub fn builtin() -> Self {
        Self::from_sources(BUILTIN_CORE, BUILTIN_STRAINS, BUILTIN_STRATA)
            .expect("built-in balance data is valid")
    }

    pub fn from_sources(core: &str, strains: &str, strata: &str) -> Result<Self, BalanceError> {
        let b = Balance {
            core: parse(core, "metabolism.ron")?,
            strains: parse(strains, "strains.ron")?,
            strata: parse(strata, "strata.ron")?,
        };
        b.validate()?;
        Ok(b)
    }

    /// Loads `dir/{metabolism,strains,strata}.ron`.
    pub fn load(dir: &Path) -> Result<Self, BalanceError> {
        let read = |name: &str| {
            let path = dir.join(name);
            std::fs::read_to_string(&path).map_err(|source| BalanceError::Io {
                path: path.display().to_string(),
                source,
            })
        };
        Self::from_sources(
            &read("metabolism.ron")?,
            &read("strains.ron")?,
            &read("strata.ron")?,
        )
    }

    /// Sanity checks that would otherwise show up as NaNs or soft-locks.
    pub fn validate(&self) -> Result<(), BalanceError> {
        let bad = |m: String| Err(BalanceError::Invalid(m));
        let m = &self.core.metabolism;
        if m.base_radius <= 0.0 || m.energy_capacity_per_biomass <= 0.0 {
            return bad("base_radius and energy capacity must be positive".into());
        }
        if !(0.0..=1.0).contains(&m.growth_fraction) {
            return bad("growth_fraction must be within 0..=1".into());
        }
        if m.max_stomach_fill < m.gorged_threshold {
            return bad("max_stomach_fill must be >= gorged_threshold".into());
        }
        let g = &self.core.size_gate;
        if !(0.0 < g.safe_ratio && g.safe_ratio <= g.struggle_max_ratio) {
            return bad("size gate needs 0 < safe_ratio <= struggle_max_ratio".into());
        }
        if self.strains.strains.is_empty() {
            return bad("no strains defined".into());
        }
        if self.strata.tiers.is_empty() {
            return bad("no tiers defined".into());
        }
        let mut prev_complete = 0.0;
        for t in &self.strata.tiers {
            if t.complete_biomass <= t.start_biomass {
                return bad(format!(
                    "tier {}: complete_biomass <= start_biomass",
                    t.tier
                ));
            }
            if t.start_biomass < prev_complete {
                return bad(format!(
                    "tier {} starts below the previous tier's goal",
                    t.tier
                ));
            }
            prev_complete = t.complete_biomass;
            for s in &t.species {
                if !(0.0 < s.radius.0 && s.radius.0 <= s.radius.1) {
                    return bad(format!("species {}: bad radius range", s.id));
                }
            }
        }
        Ok(())
    }

    pub fn strain(&self, id: &str) -> Option<&StrainParams> {
        self.strains.strains.get(id)
    }

    pub fn tier(&self, index: usize) -> Option<&TierDef> {
        self.strata.tiers.get(index)
    }

    pub fn last_tier_index(&self) -> usize {
        self.strata.tiers.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_balance_is_valid() {
        let b = Balance::builtin();
        assert!(b.strain("devourer").is_some());
        assert_eq!(b.strata.tiers.len(), 2);
        assert_eq!(b.tier(0).unwrap().biome, Biome::CraterSeafloor);
    }

    #[test]
    fn shipped_files_match_builtin() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/balance");
        assert_eq!(Balance::load(&dir).unwrap(), Balance::builtin());
    }

    #[test]
    fn validation_rejects_bad_tiers() {
        let mut b = Balance::builtin();
        b.strata.tiers[0].complete_biomass = 0.5;
        assert!(b.validate().is_err());
    }
}
