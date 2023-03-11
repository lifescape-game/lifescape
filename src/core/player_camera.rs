use std::f32::consts::FRAC_PI_2;

use bevy::{input::mouse::MouseMotion, prelude::*};
use iyes_loopless::prelude::*;
use leafwing_input_manager::prelude::ActionState;

use super::{
    action::{self, Action},
    game_state::GameState,
};

#[derive(SystemLabel)]
enum PlayerCameraSystem {
    Rotation,
    Position,
    Arm,
}

pub(super) struct PlayerCameraPlugin;

impl Plugin for PlayerCameraPlugin {
    fn build(&self, app: &mut App) {
        for state in [GameState::FamilyEditor, GameState::City, GameState::Family] {
            app.add_system(
                Self::rotation_system
                    .run_if(action::pressed(Action::RotateCamera))
                    .run_in_state(state)
                    .label(PlayerCameraSystem::Rotation),
            )
            .add_system(
                Self::arm_system
                    .run_in_state(state)
                    .label(PlayerCameraSystem::Arm),
            );

            if state != GameState::FamilyEditor {
                app.add_system(
                    Self::position_system
                        .run_in_state(state)
                        .label(PlayerCameraSystem::Position),
                )
                .add_system(
                    Self::transform_system
                        .run_in_state(state)
                        .after(PlayerCameraSystem::Rotation)
                        .after(PlayerCameraSystem::Arm)
                        .after(PlayerCameraSystem::Position),
                );
            } else {
                app.add_system(
                    Self::transform_system
                        .run_in_state(state)
                        .after(PlayerCameraSystem::Rotation)
                        .after(PlayerCameraSystem::Arm),
                );
            }
        }
    }
}

/// Interpolation multiplier for movement and camera zoom.
const INTERPOLATION_SPEED: f32 = 5.0;

impl PlayerCameraPlugin {
    fn rotation_system(
        mut motion_events: EventReader<MouseMotion>,
        mut cameras: Query<&mut OrbitRotation, With<PlayerCamera>>,
    ) {
        let mut orbit_rotation = cameras.single_mut();
        const SENSETIVITY: f32 = 0.01;
        orbit_rotation.0 -=
            SENSETIVITY * motion_events.iter().map(|event| &event.delta).sum::<Vec2>();
        orbit_rotation.y = orbit_rotation.y.clamp(0.0, FRAC_PI_2);
    }

    fn position_system(
        time: Res<Time>,
        action_state: Res<ActionState<Action>>,
        mut cameras: Query<(&mut OrbitOrigin, &Transform), With<PlayerCamera>>,
    ) {
        let (mut orbit_origin, transform) = cameras.single_mut();

        const MOVEMENT_SPEED: f32 = 10.0;
        orbit_origin.current += movement_direction(&action_state, transform.rotation)
            * time.delta_seconds()
            * MOVEMENT_SPEED;

        orbit_origin.interpolated = orbit_origin.interpolated.lerp(
            orbit_origin.current,
            time.delta_seconds() * INTERPOLATION_SPEED,
        );
    }

    fn arm_system(
        time: Res<Time>,
        action_state: Res<ActionState<Action>>,
        mut cameras: Query<&mut SpringArm, With<PlayerCamera>>,
    ) {
        let mut spring_arm = cameras.single_mut();
        spring_arm.current = (spring_arm.current - action_state.value(Action::ZoomCamera)).max(0.0);
        spring_arm.interpolated = spring_arm.interpolated
            + time.delta_seconds()
                * INTERPOLATION_SPEED
                * (spring_arm.current - spring_arm.interpolated);
    }

    fn transform_system(
        mut cameras: Query<
            (&mut Transform, &OrbitOrigin, &OrbitRotation, &SpringArm),
            With<PlayerCamera>,
        >,
    ) {
        let (mut transform, orbit_origin, orbit_rotation, spring_arm) = cameras.single_mut();
        transform.translation =
            orbit_rotation.sphere_pos() * spring_arm.interpolated + orbit_origin.interpolated;
        transform.look_at(orbit_origin.interpolated, Vec3::Y);
    }
}

fn movement_direction(action_state: &ActionState<Action>, rotation: Quat) -> Vec3 {
    let mut direction = Vec3::ZERO;
    if action_state.pressed(Action::CameraLeft) {
        direction.x -= 1.0;
    }
    if action_state.pressed(Action::CameraRight) {
        direction.x += 1.0;
    }
    if action_state.pressed(Action::CameraForward) {
        direction.z -= 1.0;
    }
    if action_state.pressed(Action::CameraBackward) {
        direction.z += 1.0;
    }

    direction = rotation * direction;
    direction.y = 0.0;

    direction.normalize_or_zero()
}

#[derive(Bundle, Default)]
pub(crate) struct PlayerCameraBundle {
    target_translation: OrbitOrigin,
    orbit_rotation: OrbitRotation,
    spring_arm: SpringArm,
    player_camera: PlayerCamera,

    #[bundle]
    camera_3d_bundle: Camera3dBundle,
}

/// The origin of a camera.
#[derive(Component, Default)]
struct OrbitOrigin {
    current: Vec3,
    interpolated: Vec3,
}

/// Camera rotation in `X` and `Z`.
#[derive(Component, Deref, DerefMut, Clone, Copy)]
struct OrbitRotation(Vec2);

impl OrbitRotation {
    fn sphere_pos(self) -> Vec3 {
        Quat::from_euler(EulerRot::YXZ, self.x, self.y, 0.0) * Vec3::Y
    }
}

impl Default for OrbitRotation {
    fn default() -> Self {
        Self(Vec2::new(0.0, 60_f32.to_radians()))
    }
}

/// Camera distance.
#[derive(Component)]
struct SpringArm {
    current: f32,
    interpolated: f32,
}

impl Default for SpringArm {
    fn default() -> Self {
        Self {
            current: 10.0,
            interpolated: 0.0,
        }
    }
}

#[derive(Component, Default)]
pub(super) struct PlayerCamera;
