//! Metaball body meshing with marching cubes.
//!
//! The organism is a sum of metaballs (`r² / d²`); the isosurface at 1.0 is
//! polygonized on a regular grid with the classic marching cubes tables.
//! Vertices are shared per grid edge, so normals (from the analytic field
//! gradient) are smooth. Runs on the AsyncComputeTaskPool; see `creature/mod.rs`.

use super::mc_tables::{EDGE_TABLE, TRI_TABLE};
use bevy::math::Vec3;
use std::collections::HashMap;

/// One metaball.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blob {
    pub center: Vec3,
    pub radius: f32,
}

pub const ISO: f32 = 1.0;

/// Mesh data independent of Bevy's `Mesh` (testable, `Send`).
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}

pub fn field(p: Vec3, blobs: &[Blob]) -> f32 {
    blobs
        .iter()
        .map(|b| b.radius * b.radius / (p - b.center).length_squared().max(1e-6))
        .sum()
}

/// Outward surface normal (negative field gradient).
pub fn normal(p: Vec3, blobs: &[Blob]) -> Vec3 {
    let mut g = Vec3::ZERO;
    for b in blobs {
        let d = p - b.center;
        let d2 = d.length_squared().max(1e-6);
        g += -2.0 * b.radius * b.radius * d / (d2 * d2);
    }
    (-g).normalize_or(Vec3::Y)
}

// Bourke corner order and edge endpoints.
const CORNERS: [(usize, usize, usize); 8] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 1, 0),
    (0, 1, 0),
    (0, 0, 1),
    (1, 0, 1),
    (1, 1, 1),
    (0, 1, 1),
];
const EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Polygonizes the blobs' isosurface on a `res`³ grid covering their bounds.
pub fn polygonize(blobs: &[Blob], res: usize) -> MeshData {
    let mut out = MeshData::default();
    if blobs.is_empty() || res < 2 {
        return out;
    }
    // Bounds: a point farther than sqrt(sum r^2) from every centre has
    // field < 1 = ISO, so the whole surface lies inside this box. The extra
    // cell of padding keeps the outermost grid layer strictly outside.
    let reach = blobs
        .iter()
        .map(|b| b.radius * b.radius)
        .sum::<f32>()
        .sqrt()
        * 1.02;
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for b in blobs {
        min = min.min(b.center - Vec3::splat(reach));
        max = max.max(b.center + Vec3::splat(reach));
    }
    let pad = (max - min) / (res as f32 - 2.0).max(1.0);
    min -= pad;
    max += pad;
    let step = (max - min) / res as f32;
    let n = res + 1;
    let idx = |x: usize, y: usize, z: usize| (z * n + y) * n + x;
    let point = |x: usize, y: usize, z: usize| {
        min + Vec3::new(x as f32 * step.x, y as f32 * step.y, z as f32 * step.z)
    };
    let mut values = vec![0.0f32; n * n * n];
    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                values[idx(x, y, z)] = field(point(x, y, z), blobs);
            }
        }
    }

    // Vertices are keyed by the global grid edge they lie on.
    let mut edge_vertex: HashMap<(usize, usize), u32> = HashMap::new();
    for z in 0..res {
        for y in 0..res {
            for x in 0..res {
                let corner_idx: [usize; 8] = std::array::from_fn(|c| {
                    let (dx, dy, dz) = CORNERS[c];
                    idx(x + dx, y + dy, z + dz)
                });
                let mut cube = 0usize;
                for (c, &ci) in corner_idx.iter().enumerate() {
                    if values[ci] > ISO {
                        cube |= 1 << c;
                    }
                }
                let edges = EDGE_TABLE[cube];
                if edges == 0 {
                    continue;
                }
                let mut verts = [0u32; 12];
                for (e, &(a, b)) in EDGES.iter().enumerate() {
                    if edges & (1 << e) == 0 {
                        continue;
                    }
                    let (ia, ib) = (corner_idx[a], corner_idx[b]);
                    let key = (ia.min(ib), ia.max(ib));
                    verts[e] = *edge_vertex.entry(key).or_insert_with(|| {
                        let (va, vb) = (values[ia], values[ib]);
                        let t = ((ISO - va) / (vb - va)).clamp(0.0, 1.0);
                        let pa = {
                            let (dx, dy, dz) = CORNERS[a];
                            point(x + dx, y + dy, z + dz)
                        };
                        let pb = {
                            let (dx, dy, dz) = CORNERS[b];
                            point(x + dx, y + dy, z + dz)
                        };
                        let p = pa.lerp(pb, t);
                        out.positions.push(p.to_array());
                        out.normals.push(normal(p, blobs).to_array());
                        (out.positions.len() - 1) as u32
                    });
                }
                let tris = &TRI_TABLE[cube];
                let mut i = 0;
                while i + 2 < 16 && tris[i] >= 0 {
                    let (a, b, c) = (
                        verts[tris[i] as usize],
                        verts[tris[i + 1] as usize],
                        verts[tris[i + 2] as usize],
                    );
                    i += 3;
                    if a == b || b == c || a == c {
                        continue; // degenerate (vertex snapped to a shared corner)
                    }
                    // Wind counter-clockwise as seen from outside (matching the normal).
                    let (pa, pb, pc) = (
                        Vec3::from_array(out.positions[a as usize]),
                        Vec3::from_array(out.positions[b as usize]),
                        Vec3::from_array(out.positions[c as usize]),
                    );
                    let face = (pb - pa).cross(pc - pa);
                    let n = Vec3::from_array(out.normals[a as usize]);
                    if face.dot(n) >= 0.0 {
                        out.indices.extend_from_slice(&[a, b, c]);
                    } else {
                        out.indices.extend_from_slice(&[a, c, b]);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_blob_is_a_closed_sphere() {
        let blobs = [Blob {
            center: Vec3::ZERO,
            radius: 1.0,
        }];
        let mesh = polygonize(&blobs, 24);
        assert!(!mesh.indices.is_empty());
        // Every vertex lies near the unit sphere.
        for p in &mesh.positions {
            let r = Vec3::from_array(*p).length();
            assert!((r - 1.0).abs() < 0.05, "r = {r}");
        }
        // Watertight: every edge is shared by exactly two triangles.
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for t in mesh.indices.chunks(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        assert!(edges.values().all(|&n| n == 2), "mesh has holes");
        // Outward winding.
        let t = &mesh.indices[0..3];
        let p: Vec<Vec3> = t
            .iter()
            .map(|&i| Vec3::from_array(mesh.positions[i as usize]))
            .collect();
        let face = (p[1] - p[0]).cross(p[2] - p[0]);
        assert!(face.dot(p[0]) > 0.0);
    }

    fn assert_watertight(mesh: &MeshData) {
        let mut edges: HashMap<(u32, u32), u32> = HashMap::new();
        for t in mesh.indices.chunks(3) {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let open = edges.values().filter(|&&n| n != 2).count();
        assert_eq!(open, 0, "{open} open edges");
    }

    #[test]
    fn every_growth_stage_body_is_closed() {
        for stage in 0..=20 {
            for seed in [1u64, 7, 42] {
                // The preset range of grid resolutions (see mesh_resolution).
                for res in [19, 32, 44] {
                    let blobs = crate::creature::body_blobs(seed, stage);
                    let mesh = polygonize(&blobs, res);
                    assert!(!mesh.indices.is_empty());
                    assert_watertight(&mesh);
                }
            }
        }
    }

    #[test]
    fn blobs_merge_into_one_surface() {
        let blobs = [
            Blob {
                center: Vec3::new(-0.5, 0.0, 0.0),
                radius: 0.6,
            },
            Blob {
                center: Vec3::new(0.5, 0.0, 0.0),
                radius: 0.6,
            },
        ];
        let mesh = polygonize(&blobs, 20);
        // The midpoint is inside the merged surface.
        assert!(field(Vec3::ZERO, &blobs) > ISO);
        assert!(mesh.positions.len() > 100);
    }
}
