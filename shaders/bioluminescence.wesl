// Bioluminescent extension of Bevy's StandardMaterial.
// Adds emission on top of PBR lighting from three parts:
//   body:  a slow breathing pulse over the whole surface
//   rim:   fresnel glow at grazing angles (membranes)
//   veins: ridged 3D noise in object space (glowing seams/capillaries)
// Used by crates/game/src/graphics/materials.rs (GlowMaterial).

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::{globals, view},
};

struct BioluminescenceParams {
    // rgb: emissive color (linear), a: intensity
    glow: vec4<f32>,
    // x: pulse speed (Hz), y: pulse depth (0..1), z: rim power, w: phase offset
    pulse: vec4<f32>,
    // x: vein scale (1/m), y: vein sharpness (0..1), z: body amount, w: rim amount
    pattern: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> bio: BioluminescenceParams;

const TAU: f32 = 6.28318530718;

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.3183099 + vec3<f32>(0.1, 0.2, 0.3)) * 17.0;
    return fract(q.x * q.y * q.z * (q.x + q.y + q.z));
}

// Trilinear value noise in [0, 1].
fn value_noise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let n000 = hash3(i);
    let n100 = hash3(i + vec3<f32>(1.0, 0.0, 0.0));
    let n010 = hash3(i + vec3<f32>(0.0, 1.0, 0.0));
    let n110 = hash3(i + vec3<f32>(1.0, 1.0, 0.0));
    let n001 = hash3(i + vec3<f32>(0.0, 0.0, 1.0));
    let n101 = hash3(i + vec3<f32>(1.0, 0.0, 1.0));
    let n011 = hash3(i + vec3<f32>(0.0, 1.0, 1.0));
    let n111 = hash3(i + vec3<f32>(1.0, 1.0, 1.0));
    let x00 = mix(n000, n100, u.x);
    let x10 = mix(n010, n110, u.x);
    let x01 = mix(n001, n101, u.x);
    let x11 = mix(n011, n111, u.x);
    return mix(mix(x00, x10, u.y), mix(x01, x11, u.y), u.z);
}

// Ridged noise: thin bright lines where the noise crosses 0.5.
fn veins(p: vec3<f32>, sharpness: f32) -> f32 {
    let n = value_noise(p) * 0.65 + value_noise(p * 2.13) * 0.35;
    let ridge = 1.0 - abs(n * 2.0 - 1.0);
    return pow(ridge, mix(2.0, 24.0, sharpness));
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color,
    );

    // Breathing pulse, phase-shifted along the surface so it travels.
    let wave_phase = dot(in.world_position.xyz, vec3<f32>(0.9, 1.3, 0.7));
    let pulse = 1.0 - bio.pulse.y
        * (0.5 + 0.5 * sin(globals.time * bio.pulse.x * TAU + bio.pulse.w + wave_phase));

    // Fresnel rim: membranes glow brightest at grazing angles.
    let view_dir = normalize(view.world_position.xyz - in.world_position.xyz);
    let n = normalize(pbr_input.N);
    let rim = pow(1.0 - saturate(dot(n, view_dir)), bio.pulse.z);

    var vein = 0.0;
    if (bio.pattern.x > 0.0) {
        vein = veins(in.world_position.xyz * bio.pattern.x, bio.pattern.y);
    }

    let shape = bio.pattern.z * pulse + bio.pattern.w * rim + vein * (0.5 + 0.5 * pulse);
    let emission = bio.glow.rgb * bio.glow.a * shape;
    // emissive.a is the exposure weight (0 = emission ignores camera exposure,
    // which is what self-lit organisms want). Keep the material's value.
    pbr_input.material.emissive = vec4<f32>(
        pbr_input.material.emissive.rgb + emission,
        pbr_input.material.emissive.a,
    );

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
