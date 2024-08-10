use bevy::{math::Vec3Swizzles, prelude::*};
use bevy_replicon::prelude::*;
use leafwing_input_manager::common_conditions::action_just_pressed;

use super::{LotCreate, LotEventConfirmed, LotTool, LotVertices, UnconfirmedLot};
use crate::{
    game_world::{city::ActiveCity, player_camera::CameraCaster},
    settings::Action,
};

pub(super) struct CreatingLotPlugin;

impl Plugin for CreatingLotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PreUpdate,
            Self::end_creation
                .after(ClientSet::Receive)
                .run_if(in_state(LotTool::Create))
                .run_if(on_event::<LotEventConfirmed>()),
        )
        .add_systems(
            Update,
            (
                Self::start_creation
                    .run_if(action_just_pressed(Action::Confirm))
                    .run_if(not(any_with_component::<CreatingLot>)),
                Self::set_vertex_position,
                Self::confirm.run_if(action_just_pressed(Action::Confirm)),
                Self::end_creation.run_if(action_just_pressed(Action::Cancel)),
            )
                .run_if(in_state(LotTool::Create)),
        );
    }
}

impl CreatingLotPlugin {
    fn start_creation(
        camera_caster: CameraCaster,
        mut commands: Commands,
        cities: Query<Entity, With<ActiveCity>>,
    ) {
        if let Some(point) = camera_caster.intersect_ground() {
            info!("starting placing lot");
            // Spawn with two the same vertices because we edit the last one on cursor movement.
            commands.entity(cities.single()).with_children(|parent| {
                parent.spawn((
                    StateScoped(LotTool::Create),
                    LotVertices(vec![point.xz(); 2].into()),
                    CreatingLot,
                ));
            });
        }
    }

    fn set_vertex_position(
        camera_caster: CameraCaster,
        mut creating_lots: Query<&mut LotVertices, (With<CreatingLot>, Without<UnconfirmedLot>)>,
    ) {
        if let Ok(mut lot_vertices) = creating_lots.get_single_mut() {
            if let Some(point) = camera_caster.intersect_ground().map(|hover| hover.xz()) {
                let first_vertex = *lot_vertices
                    .first()
                    .expect("vertices should have at least 2 vertices");
                let last_vertex = lot_vertices.last_mut().unwrap();

                const SNAP_DELTA: f32 = 0.1;
                let delta = first_vertex - point;
                if delta.x.abs() <= SNAP_DELTA && delta.y.abs() <= SNAP_DELTA {
                    trace!("snapping vertex position to last vertex `{last_vertex:?}`");
                    *last_vertex = first_vertex;
                } else {
                    trace!("updating vertex position to `{point:?}`");
                    *last_vertex = point;
                }
            }
        }
    }

    fn confirm(
        mut create_events: EventWriter<LotCreate>,
        mut creating_lots: Query<&mut LotVertices, (With<CreatingLot>, Without<UnconfirmedLot>)>,
        cities: Query<Entity, With<ActiveCity>>,
    ) {
        if let Ok(mut lot_vertices) = creating_lots.get_single_mut() {
            let first_vertex = *lot_vertices
                .first()
                .expect("vertices should have at least 2 vertices");
            let last_vertex = *lot_vertices.last().unwrap();
            if first_vertex == last_vertex {
                info!("confirming lot creation");
                create_events.send(LotCreate {
                    polygon: lot_vertices.0.clone(),
                    city_entity: cities.single(),
                });
            } else {
                info!("confirming lot point");
                lot_vertices.push(last_vertex);
            }
        }
    }

    fn end_creation(mut commands: Commands, creating_lots: Query<Entity, With<CreatingLot>>) {
        if let Ok(entity) = creating_lots.get_single() {
            info!("ending lot creation");
            commands.entity(entity).despawn();
        }
    }
}

#[derive(Component)]
pub struct CreatingLot;
