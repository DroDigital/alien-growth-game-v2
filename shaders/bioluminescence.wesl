// Bioluminescent extension of Bevy's StandardMaterial.
// Adds emission on top of PBR lighting from three parts:
//   body:  a slow breathing pulse over the whole surface
//   rim:   fresnel glow at grazing angles (membranes)
//   veins: ridged 3D noise in object space (glowing seams/capillaries)
// Used by crates/game/src/graphics/materials.rs (GlowMaterial, and CreatureSkin
// which adds the vertex wobble stage below).

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{Vertex, VertexOutput, FragmentOutput, decompress_vertex},
    mesh_functions,
    mesh_view_bindings::{globals, view},
    view_transformations::position_world_to_clip,
};

struct BioluminescenceParams {
    // rgb: emissive color (linear), a: intensity
    glow: vec4<f32>,
    // x: pulse speed (Hz), y: pulse depth (0..1), z: rim power, w: phase offset
    pulse: vec4<f32>,
    // x: vein scale (1/m), y: vein sharpness (0..1), z: body amount, w: rim amount
    pattern: vec4<f32>,
    // x: wobble amplitude (local units), y: spatial frequency, z: speed, w: pulse amplitude
    wobble: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100)
var<uniform> bio: BioluminescenceParams;

const TAU: f32 = 6.28318530718;

// Vertex stage for the creature skin (CreatureSkin): the standard mesh vertex
// path plus displacement along the normal. Static meshes (GlowMaterial) use
// Bevy's default vertex shader and never run this.
@vertex
fn vertex(vertex_in: Vertex) -> VertexOutput {
    var out: VertexOutput;
    var vertex = decompress_vertex(vertex_in, vertex_in.instance_index);
    let world_from_local = mesh_functions::get_world_from_local(vertex_in.instance_index);

    let p = vertex.position;
    let t = globals.time;
    let freq = bio.wobble.y;
    let wobble = sin(dot(p, vec3<f32>(1.7, 2.3, 1.3)) * freq + t * bio.wobble.z)
        + 0.5 * sin(dot(p, vec3<f32>(-2.1, 0.7, 1.9)) * freq * 1.7 - t * bio.wobble.z * 1.3);
    let pulse = sin(t * bio.pulse.x * TAU) * bio.wobble.w;
@if(VERTEX_NORMALS)
    vertex.position = p + vertex.normal * (wobble * bio.wobble.x + pulse);

@if(VERTEX_NORMALS)
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex_in.instance_index);
@if(VERTEX_POSITIONS)
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
@if(VERTEX_POSITIONS)
    out.position = position_world_to_clip(out.world_position.xyz);
@if(VERTEX_UVS_A)
    out.uv = vertex.uv;
@if(VERTEX_UVS_B)
    out.uv_b = vertex.uv_b;
@if(VERTEX_TANGENTS)
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(world_from_local, vertex.tangent, vertex_in.instance_index);
@if(VERTEX_COLORS)
    out.color = vertex.color;
@if(VERTEX_OUTPUT_INSTANCE_INDEX)
    out.instance_index = vertex_in.instance_index;
@if(VISIBILITY_RANGE_DITHER)
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex_in.instance_index, world_from_local[3]);
    return out;
}

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
