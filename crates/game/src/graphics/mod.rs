//! Graphics plugin: camera rig, render scale, quality settings, shared
//! materials and the procedural environment.
//!
//! Camera rig:
//! - `WorldCamera` (Camera3d, order 0) renders the 3D scene into an offscreen
//!   image at `RenderSettings::render_resolution(output)` (output capped to the
//!   preset's reference height, × render_scale), with HDR, tonemapping, bloom,
//!   AA and the other per-preset effects.
//! - `PresentCamera` (Camera2d, order 1) draws that image full-screen (bilinear
//!   upscale) plus the UI at native resolution. It targets the window, or an
//!   offscreen output image in headless modes (screenshot, warm-up).

pub mod env;
pub mod materials;
pub mod pipelines;

use crate::boot::{ActiveRenderSettings, LaunchContext};
use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::smaa::Smaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::camera::{Hdr, RenderTarget, ShadowLodOrigin};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{DirectionalLightShadowMap, VolumetricFog};
use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel};
use bevy::post_process::bloom::Bloom;
use bevy::post_process::dof::DepthOfField;
use bevy::post_process::motion_blur::MotionBlur;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::window::PrimaryWindow;
use game_core::config::{AntiAliasing, RenderSettings, SsaoQuality};

/// Default output resolution for headless modes.
pub const HEADLESS_DEFAULT_RESOLUTION: (u32, u32) = (1920, 1080);

/// Format of the offscreen scene and output images (tonemapped LDR).
const TARGET_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// Builds the RenderPlugin with the requested backend.
pub fn render_plugin(backend: Option<&str>) -> RenderPlugin {
    let mut wgpu = WgpuSettings::default();
    // `None` in WgpuSettings means "no renderer", so only override when the
    // player picked a backend; otherwise keep wgpu's platform defaults
    // (D3D12/Vulkan on Windows, Metal on macOS, Vulkan/GL on Linux).
    let chosen = match backend.map(str::to_ascii_lowercase).as_deref() {
        Some("dx12") | Some("d3d12") => Some(Backends::DX12),
        Some("vulkan") | Some("vk") => Some(Backends::VULKAN),
        Some("metal") => Some(Backends::METAL),
        Some("gl") | Some("opengl") => Some(Backends::GL),
        _ => None,
    };
    if chosen.is_some() {
        wgpu.backends = chosen;
    }
    RenderPlugin {
        render_creation: RenderCreation::Automatic(Box::new(wgpu)),
        ..default()
    }
}

/// The 3D scene camera.
#[derive(Component)]
pub struct WorldCamera;

/// The camera that composites the scene image and draws UI.
#[derive(Component)]
pub struct PresentCamera;

/// Full-screen UI node showing the upscaled scene image.
#[derive(Component)]
struct SceneView;

/// Offscreen images of the camera rig.
#[derive(Resource, Debug, Clone)]
pub struct RenderTargets {
    /// 3D scene at render resolution.
    pub scene: Handle<Image>,
    /// Final composited output when headless (None when presenting to a window).
    pub output: Option<Handle<Image>>,
    /// Output resolution in physical pixels.
    pub output_size: UVec2,
    /// Current scene render resolution.
    pub scene_size: UVec2,
}

pub struct GraphicsPlugin;

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            materials::MaterialsPlugin,
            env::EnvironmentPlugin,
            pipelines::PipelineStatusPlugin,
            bevy_hanabi::HanabiPlugin,
        ))
        .add_systems(PreStartup, setup_camera_rig)
        .add_systems(
            Update,
            (
                track_window_size,
                sync_scene_target_size,
                apply_render_settings.run_if(
                    resource_changed::<ActiveRenderSettings>
                        .or_else(any_with_component::<NeedsQualityApply>),
                ),
            )
                .chain(),
        );
    }
}

/// Marker: settings must be (re)applied to this camera.
#[derive(Component)]
struct NeedsQualityApply;

fn scene_size(output: UVec2, settings: &RenderSettings) -> UVec2 {
    let (w, h) = settings.render_resolution((output.x, output.y));
    UVec2::new(w, h)
}

fn make_target(size: UVec2) -> Image {
    let mut image = Image::new_target_texture(size.x, size.y, TARGET_FORMAT, None);
    // COPY_SRC lets the screenshot system read it back.
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    image
}

fn setup_camera_rig(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    ctx: Res<LaunchContext>,
    settings: Res<ActiveRenderSettings>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let headless = ctx.mode.is_headless();
    let output_size = if headless {
        let (w, h) = ctx.cli.resolution.unwrap_or(HEADLESS_DEFAULT_RESOLUTION);
        UVec2::new(w, h)
    } else {
        windows
            .single()
            .map(|w| w.physical_size())
            .unwrap_or(UVec2::new(1600, 900))
    };
    let scene_size = scene_size(output_size, &settings);
    let scene = images.add(make_target(scene_size));
    let output = headless.then(|| images.add(make_target(output_size)));

    commands.spawn((
        Name::new("WorldCamera"),
        WorldCamera,
        NeedsQualityApply,
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        RenderTarget::from(scene.clone()),
        // Renders to an image, so tell shadow LOD which camera to center on.
        ShadowLodOrigin,
        Hdr,
        Tonemapping::AgX,
        Msaa::Off,
        Transform::from_xyz(0.0, 2.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let present_target = match &output {
        Some(img) => RenderTarget::from(img.clone()),
        None => RenderTarget::default(),
    };
    commands.spawn((
        Name::new("PresentCamera"),
        PresentCamera,
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        present_target,
        Msaa::Off,
        IsDefaultUiCamera,
    ));

    commands.spawn((
        Name::new("SceneView"),
        SceneView,
        ImageNode::new(scene.clone()),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        GlobalZIndex(-1000),
    ));

    commands.insert_resource(RenderTargets {
        scene,
        output,
        output_size,
        scene_size,
    });
}

fn track_window_size(
    mut targets: ResMut<RenderTargets>,
    windows: Query<&Window, (With<PrimaryWindow>, Changed<Window>)>,
) {
    if targets.output.is_some() {
        return;
    }
    if let Ok(w) = windows.single() {
        let size = w.physical_size().max(UVec2::ONE);
        if size != targets.output_size {
            targets.output_size = size;
        }
    }
}

fn sync_scene_target_size(
    mut targets: ResMut<RenderTargets>,
    settings: Res<ActiveRenderSettings>,
    mut images: ResMut<Assets<Image>>,
) {
    let want = scene_size(targets.output_size, &settings);
    if want == targets.scene_size {
        return;
    }
    if let Some(mut image) = images.get_mut(&targets.scene) {
        image.resize(bevy::render::render_resource::Extent3d {
            width: want.x,
            height: want.y,
            depth_or_array_layers: 1,
        });
        debug!("scene target resized to {}x{}", want.x, want.y);
        targets.scene_size = want;
    }
}

/// Inserts/removes per-preset camera effects on the world camera.
fn apply_render_settings(
    mut commands: Commands,
    settings: Res<ActiveRenderSettings>,
    cameras: Query<Entity, With<WorldCamera>>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
) {
    let s: &RenderSettings = &settings;
    shadow_map.size = s.shadow_map_size as usize;
    for cam in &cameras {
        let mut e = commands.entity(cam);
        e.remove::<NeedsQualityApply>();

        e.remove::<(TemporalAntiAliasing, Smaa, Fxaa)>();
        match s.anti_aliasing {
            AntiAliasing::Taa => {
                e.insert(TemporalAntiAliasing::default());
            }
            AntiAliasing::Smaa => {
                e.insert(Smaa::default());
            }
            AntiAliasing::Fxaa => {
                e.insert(Fxaa::default());
            }
            AntiAliasing::Off => {}
        }

        if s.bloom {
            e.insert(Bloom::NATURAL);
        } else {
            e.remove::<Bloom>();
        }

        match ssao_level(s.ssao) {
            Some(quality_level) => {
                e.insert(ScreenSpaceAmbientOcclusion {
                    quality_level,
                    ..default()
                });
            }
            None => {
                e.remove::<ScreenSpaceAmbientOcclusion>();
            }
        }

        if s.volumetric_fog {
            e.insert(VolumetricFog {
                ambient_intensity: 0.0,
                step_count: s.volumetric_fog_steps,
                ..default()
            });
        } else {
            e.remove::<VolumetricFog>();
        }

        if s.depth_of_field {
            e.insert(DepthOfField {
                focal_distance: 8.0,
                aperture_f_stops: 2.8,
                ..default()
            });
        } else {
            e.remove::<DepthOfField>();
        }

        if s.motion_blur {
            e.insert(MotionBlur::default());
        } else {
            e.remove::<MotionBlur>();
        }
        // SSR (needs the deferred path) and irradiance volumes (need baked
        // data per stratum) are wired in the M2 visual pass.
    }
}

fn ssao_level(q: SsaoQuality) -> Option<ScreenSpaceAmbientOcclusionQualityLevel> {
    use ScreenSpaceAmbientOcclusionQualityLevel as L;
    match q {
        SsaoQuality::Off => None,
        SsaoQuality::Low => Some(L::Low),
        SsaoQuality::Medium => Some(L::Medium),
        SsaoQuality::High => Some(L::High),
        SsaoQuality::Ultra => Some(L::Ultra),
    }
}
