//! Size-scaled follow camera for runs: distance and field of view grow with
//! the organism, mouse/right stick orbit, terrain clearance, trauma shake and
//! a zoom-out multiplier used by the tier-up cinematic.

use super::WorldCamera;
use crate::boot::PlayerSettings;
use crate::creature::Trauma;
use crate::input::{GameAction, PlayerInput};
use bevy::camera::Projection;
use bevy::prelude::*;
use game_core::sim::{Arena, Player};
use game_core::states::{AppState, RunState};
use leafwing_input_manager::prelude::ActionState;

#[derive(Component, Debug, Clone)]
pub struct FollowCamera {
    pub yaw: f32,
    pub pitch: f32,
    /// Extra distance multiplier (1 = normal; the tier-up cinematic raises it).
    pub zoom_out: f32,
    smoothed_radius: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.35,
            zoom_out: 1.0,
            smoothed_radius: 0.25,
        }
    }
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InRun), attach)
            .add_systems(OnExit(AppState::InRun), detach)
            .add_systems(
                PostUpdate,
                follow
                    .run_if(in_state(AppState::InRun))
                    .before(TransformSystems::Propagate),
            );
    }
}

fn attach(mut commands: Commands, cams: Query<Entity, With<WorldCamera>>) {
    for e in &cams {
        commands
            .entity(e)
            .remove::<crate::scenes::OrbitShot>()
            .insert(FollowCamera::default());
    }
}

fn detach(mut commands: Commands, mut cams: Query<(Entity, &mut Projection), With<FollowCamera>>) {
    for (e, mut proj) in &mut cams {
        commands.entity(e).remove::<FollowCamera>();
        if let Projection::Perspective(p) = proj.as_mut() {
            p.fov = std::f32::consts::FRAC_PI_4;
        }
    }
}

fn follow(
    time: Res<Time<Real>>,
    settings: Res<PlayerSettings>,
    state: Option<Res<State<RunState>>>,
    arena: Option<Res<Arena>>,
    mut trauma: ResMut<Trauma>,
    actions: Query<&ActionState<GameAction>, With<PlayerInput>>,
    player: Query<(&Transform, &game_core::sim::Radius), (With<Player>, Without<FollowCamera>)>,
    mut cams: Query<(&mut FollowCamera, &mut Transform, &mut Projection)>,
) {
    let dt = time.delta_secs().min(0.1);
    let Ok((target, radius)) = player.single() else {
        return;
    };
    let playing = state.is_some_and(|s| *s.get() == RunState::Playing);
    for (mut cam, mut t, mut proj) in &mut cams {
        if playing && let Ok(a) = actions.single() {
            let look = a.axis_pair(&GameAction::Look);
            cam.yaw -= look.x * 0.0035;
            cam.pitch = (cam.pitch + look.y * 0.0025).clamp(-0.5, 1.25);
        }
        // Smooth size changes so growth reads as a camera pull-back.
        cam.smoothed_radius += (radius.0 - cam.smoothed_radius) * (1.0 - (-2.0 * dt).exp());
        let r = cam.smoothed_radius.max(0.05);
        let distance = r * 7.5 * cam.zoom_out + 0.6;
        let rot = Quat::from_euler(EulerRot::YXZ, cam.yaw, -cam.pitch, 0.0);
        let mut eye = target.translation + rot * Vec3::new(0.0, 0.0, distance);
        if let Some(arena) = &arena {
            let floor = arena.floor(eye.x, eye.z) + r * 0.6 + 0.1;
            eye.y = eye.y.max(floor);
        }
        let follow = 1.0 - (-10.0 * dt).exp();
        t.translation = t.translation.lerp(eye, follow);
        t.look_at(target.translation, Vec3::Y);

        // Field of view widens as the organism grows (log-scaled).
        let growth = ((r / 0.25).log2() / 6.0).clamp(0.0, 1.0);
        if let Projection::Perspective(p) = proj.as_mut() {
            p.fov = (55.0 + 15.0 * growth + 10.0 * (cam.zoom_out - 1.0).min(1.0)).to_radians();
        }

        // Trauma shake (accessibility: off when camera shake is disabled).
        if settings.accessibility.camera_shake && trauma.0 > 0.0 {
            let s = trauma.0 * trauma.0;
            let tt = time.elapsed_secs() * 40.0;
            let offset =
                Vec3::new((tt * 1.3).sin(), (tt * 1.7).cos(), (tt * 0.9).sin()) * s * r * 0.35;
            t.translation += offset;
        }
        trauma.0 = (trauma.0 - dt * 1.5).max(0.0);
    }
}
