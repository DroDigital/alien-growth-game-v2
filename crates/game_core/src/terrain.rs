//! Seeded terrain height fields. The simulation keeps creatures above the
//! floor with these, and the renderer meshes the same function, so what you
//! see is what you collide with.

use crate::balance::Biome;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

/// Radius of the impact crater at the centre of the Crater Seafloor biome.
pub const CRATER_RADIUS: f32 = 7.0;

#[derive(Clone)]
pub struct Terrain {
    pub biome: Biome,
    pub seed: u32,
    fbm: Fbm<Perlin>,
}

impl std::fmt::Debug for Terrain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Terrain")
            .field("biome", &self.biome)
            .field("seed", &self.seed)
            .finish()
    }
}

impl Terrain {
    pub fn new(biome: Biome, seed: u32) -> Self {
        Self {
            biome,
            seed,
            fbm: Fbm::<Perlin>::new(seed).set_octaves(5),
        }
    }

    fn n(&self, x: f32, z: f32, freq: f32) -> f32 {
        self.fbm
            .get([x as f64 * freq as f64, z as f64 * freq as f64]) as f32
    }

    /// Floor height at (x, z).
    pub fn height(&self, x: f32, z: f32) -> f32 {
        match self.biome {
            Biome::CraterSeafloor => {
                let dunes = self.n(x, z, 0.035) * 2.2;
                let r = (x * x + z * z).sqrt();
                let bowl = if r < CRATER_RADIUS {
                    let t = r / CRATER_RADIUS;
                    -1.6 * (1.0 - t * t)
                } else {
                    0.0
                };
                let rim = 0.9 * (-((r - CRATER_RADIUS) / 1.6).powi(2)).exp();
                let dune_mask = (r / (CRATER_RADIUS * 1.4)).clamp(0.25, 1.0);
                dunes * dune_mask + bowl + rim
            }
            Biome::CoralReef => {
                // Broad sand flats cut by channels, sloping gently toward deep water.
                let broad = self.n(x, z, 0.008) * 9.0;
                let detail = self.n(x + 300.0, z - 120.0, 0.05) * 1.2;
                let channel = (self.n(x - 50.0, z + 80.0, 0.012).abs() * 8.0).min(1.0);
                broad + detail - (1.0 - channel) * 3.0 - z * 0.03
            }
        }
    }

    /// Gradient-based normal at (x, z) (for meshing and placing props).
    pub fn normal(&self, x: f32, z: f32) -> [f32; 3] {
        let e = 0.25;
        let dx = self.height(x + e, z) - self.height(x - e, z);
        let dz = self.height(x, z + e) - self.height(x, z - e);
        let (nx, ny, nz) = (-dx, 2.0 * e, -dz);
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        [nx / len, ny / len, nz / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crater_is_lower_than_rim() {
        let t = Terrain::new(Biome::CraterSeafloor, 1);
        assert!(t.height(0.0, 0.0) < t.height(CRATER_RADIUS, 0.0) - 1.0);
    }

    #[test]
    fn deterministic_per_seed() {
        let a = Terrain::new(Biome::CoralReef, 7);
        let b = Terrain::new(Biome::CoralReef, 7);
        let c = Terrain::new(Biome::CoralReef, 8);
        assert_eq!(a.height(12.5, -3.0), b.height(12.5, -3.0));
        assert_ne!(a.height(12.5, -3.0), c.height(12.5, -3.0));
        let n = a.normal(3.0, 4.0);
        assert!((n[0] * n[0] + n[1] * n[1] + n[2] * n[2] - 1.0).abs() < 1e-4);
    }
}
