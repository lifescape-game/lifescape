pub(crate) mod side_snap;
pub(crate) mod wall_snap;

use std::{
    f32::consts::{FRAC_PI_2, FRAC_PI_4, PI},
    fmt::Debug,
};

use bevy::{
    asset::AssetPath,
    color::palettes::css::{RED, WHITE},
    prelude::*,
    scene,
};
use bevy_xpbd_3d::prelude::*;
use leafwing_input_manager::common_conditions::action_just_pressed;

use crate::{
    asset::info::object_info::ObjectInfo,
    game_world::{
        city::CityMode,
        commands_history::{CommandsHistory, DespawnOnConfirm},
        family::BuildingMode,
        hover::{HoverEnabled, Hovered},
        object::{Object, ObjectCommand},
        player_camera::CameraCaster,
    },
    settings::Action,
};
use side_snap::SideSnapPlugin;
use wall_snap::WallSnapPlugin;

pub(super) struct PlacingObjectPlugin;

impl Plugin for PlacingObjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WallSnapPlugin)
            .add_plugins(SideSnapPlugin)
            .add_systems(
                Update,
                (
                    (
                        Self::pick
                            .run_if(action_just_pressed(Action::Confirm))
                            .run_if(not(any_with_component::<PlacingObject>)),
                        Self::delete.run_if(action_just_pressed(Action::Delete)),
                        Self::end_placing.run_if(action_just_pressed(Action::Cancel)),
                    ),
                    (
                        Self::rotate.run_if(action_just_pressed(Action::RotateObject)),
                        Self::apply_position,
                        Self::check_collision,
                        Self::confirm.run_if(action_just_pressed(Action::Confirm)),
                    )
                        .chain(),
                )
                    .run_if(in_state(CityMode::Objects).or_else(in_state(BuildingMode::Objects))),
            )
            .add_systems(
                SpawnScene,
                Self::update_materials
                    .after(scene::scene_spawner_system)
                    .run_if(in_state(CityMode::Objects).or_else(in_state(BuildingMode::Objects))),
            )
            .add_systems(
                PostUpdate,
                (Self::init, Self::ensure_single)
                    .run_if(in_state(CityMode::Objects).or_else(in_state(BuildingMode::Objects))),
            );
    }
}

impl PlacingObjectPlugin {
    fn pick(
        mut commands: Commands,
        hovered: Query<(Entity, &Parent), (With<Object>, With<Hovered>)>,
    ) {
        if let Ok((object_entity, parent)) = hovered.get_single() {
            info!("picking object `{object_entity}`");
            commands.entity(**parent).with_children(|parent| {
                parent.spawn(PlacingObject::moving(object_entity));
            });
        }
    }

    /// Inserts necessary components to trigger object initialization.
    fn init(
        mut commands: Commands,
        camera_caster: CameraCaster,
        mut hover_enabled: ResMut<HoverEnabled>,
        asset_server: Res<AssetServer>,
        placing_objects: Query<(Entity, &PlacingObject), Without<Object>>,
        objects: Query<(&Position, &Rotation, &Object)>,
    ) {
        let Some((placing_entity, &placing_object)) = placing_objects.iter().last() else {
            return;
        };

        debug!(
            "initializing placing object `{:?}` for `{placing_entity}`",
            placing_object.kind
        );
        match placing_object.kind {
            PlacingObjectKind::Spawning(id) => {
                let info_path = asset_server
                    .get_path(id)
                    .expect("info should always come from file");

                // Rotate towards camera and round to the nearest cardinal direction.
                let (transform, _) = camera_caster.cameras.single();
                let (_, rotation, _) = transform.to_scale_rotation_translation();
                let (y, ..) = rotation.to_euler(EulerRot::YXZ);
                let rounded_angle = (y / FRAC_PI_2).round() * FRAC_PI_2 - PI;

                commands
                    .entity(placing_entity)
                    .insert(PlacingInitBundle::spawning(
                        info_path.into_owned(),
                        rounded_angle,
                    ));
            }
            PlacingObjectKind::Moving(object_entity) => {
                let (&position, &rotation, info_path) = objects
                    .get(object_entity)
                    .expect("moving object should have scene and path");

                let offset = camera_caster
                    .intersect_ground()
                    .map(|point| *position - point)
                    .unwrap_or(*position);

                commands
                    .entity(placing_entity)
                    .insert(PlacingInitBundle::moving(
                        info_path.0.clone(),
                        CursorOffset(offset),
                        position,
                        rotation,
                    ));
            }
        }

        hover_enabled.0 = false;
    }

    fn rotate(mut placing_objects: Query<(&mut Rotation, &PlacingObject)>) {
        if let Ok((mut rotation, object)) = placing_objects.get_single_mut() {
            **rotation *=
                Quat::from_axis_angle(Vec3::Y, object.rotation_limit.unwrap_or(FRAC_PI_4));

            debug!(
                "rotating placing object to '{}'",
                rotation.to_euler(EulerRot::YXZ).0.to_degrees()
            );
        }
    }

    fn apply_position(
        camera_caster: CameraCaster,
        mut placing_objects: Query<(&mut Position, &CursorOffset)>,
    ) {
        if let Ok((mut position, cursor_offset)) = placing_objects.get_single_mut() {
            if let Some(point) = camera_caster.intersect_ground() {
                **position = point + cursor_offset.0;
            }
        }
    }

    fn check_collision(
        mut placing_objects: Query<(&mut PlaceState, &PlacingObject, &CollidingEntities)>,
    ) {
        if let Ok((mut state, &placing_object, colliding_entities)) =
            placing_objects.get_single_mut()
        {
            let mut collides = !colliding_entities.is_empty();
            if let PlacingObjectKind::Moving(entity) = placing_object.kind {
                if colliding_entities.len() == 1 && colliding_entities.contains(&entity) {
                    // Ignore collision with the moving object.
                    collides = false;
                }
            }

            if state.collides != collides {
                debug!("setting collides to `{collides:?}`");
                state.collides = collides;
            }
        }
    }

    fn update_materials(
        mut materials: ResMut<Assets<StandardMaterial>>,
        placing_objects: Query<(Entity, &PlaceState), Or<(Added<Children>, Changed<PlaceState>)>>,
        children: Query<&Children>,
        mut material_handles: Query<&mut Handle<StandardMaterial>>,
    ) {
        if let Ok((placing_entity, state)) = placing_objects.get_single() {
            let color = if state.placeable() {
                WHITE.into()
            } else {
                RED.into()
            };

            let mut iter =
                material_handles.iter_many_mut(children.iter_descendants(placing_entity));
            while let Some(mut material_handle) = iter.fetch_next() {
                let material = materials
                    .get(&*material_handle)
                    .expect("material handle should be valid");

                // If color matches, assume that we don't need any update.
                if material.base_color == color {
                    return;
                }

                let mut material = material.clone();
                material.base_color = color;
                material.alpha_mode = AlphaMode::Add;
                *material_handle = materials.add(material);
            }
        }
    }

    fn confirm(
        mut commands: Commands,
        mut history: CommandsHistory,
        asset_server: Res<AssetServer>,
        mut hover_enabled: ResMut<HoverEnabled>,
        placing_objects: Query<(Entity, &Position, &Rotation, &PlacingObject, &PlaceState)>,
    ) {
        if let Ok((entity, position, rotation, &placing_object, state)) =
            placing_objects.get_single()
        {
            if state.placeable() {
                let id = match placing_object.kind {
                    PlacingObjectKind::Spawning(id) => {
                        let info_path = asset_server
                            .get_path(id)
                            .expect("info should always come from file");
                        history.push_pending(ObjectCommand::Buy {
                            info_path: info_path.into_owned(),
                            position: **position,
                            rotation: **rotation,
                        })
                    }
                    PlacingObjectKind::Moving(entity) => {
                        history.push_pending(ObjectCommand::Move {
                            entity,
                            position: **position,
                            rotation: **rotation,
                        })
                    }
                };
                hover_enabled.0 = true;

                commands
                    .entity(entity)
                    .insert(DespawnOnConfirm(id))
                    .remove::<(PlacingObject, PlacingInitBundle)>();

                info!("confirming `{:?}`", placing_object.kind);
            }
        }
    }

    fn delete(
        mut commands: Commands,
        mut history: CommandsHistory,
        mut hover_enabled: ResMut<HoverEnabled>,
        placing_objects: Query<(Entity, &PlacingObject)>,
    ) {
        if let Ok((entity, &placing_object)) = placing_objects.get_single() {
            info!("deleting placing object");
            if let PlacingObjectKind::Moving(entity) = placing_object.kind {
                history.push_pending(ObjectCommand::Sell { entity });
            }
            commands.entity(entity).despawn_recursive();
            hover_enabled.0 = true;
        }
    }

    fn end_placing(
        mut commands: Commands,
        mut hover_enabled: ResMut<HoverEnabled>,
        placing_objects: Query<Entity, With<PlacingObject>>,
    ) {
        if let Ok(placing_entity) = placing_objects.get_single() {
            info!("ending placing");
            hover_enabled.0 = true;
            commands.entity(placing_entity).despawn_recursive();
        }
    }

    fn ensure_single(
        mut commands: Commands,
        new_placing_objects: Query<Entity, Added<PlacingObject>>,
        placing_objects: Query<Entity, With<PlacingObject>>,
    ) {
        if let Some(new_entity) = new_placing_objects.iter().last() {
            for placing_entity in &placing_objects {
                if placing_entity != new_entity {
                    debug!("removing previous placing object `{placing_entity}`");
                    commands.entity(placing_entity).despawn_recursive();
                }
            }
        }
    }
}

/// Marks an entity as an object that should be moved with cursor to preview spawn position.
#[derive(Component, Clone, Copy)]
pub struct PlacingObject {
    kind: PlacingObjectKind,
    rotation_limit: Option<f32>,
}

impl PlacingObject {
    pub fn spawning(id: AssetId<ObjectInfo>) -> Self {
        Self {
            kind: PlacingObjectKind::Spawning(id),
            rotation_limit: None,
        }
    }

    fn moving(entity: Entity) -> Self {
        Self {
            kind: PlacingObjectKind::Moving(entity),
            rotation_limit: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PlacingObjectKind {
    Spawning(AssetId<ObjectInfo>),
    Moving(Entity),
}

/// Additional components that needed for [`PlacingObject`].
#[derive(Bundle)]
struct PlacingInitBundle {
    object: Object,
    cursor_offset: CursorOffset,
    position: Position,
    rotation: Rotation,
    state: PlaceState,
}

impl PlacingInitBundle {
    fn spawning(info_path: AssetPath<'static>, angle: f32) -> Self {
        Self {
            object: Object(info_path),
            cursor_offset: Default::default(),
            position: Default::default(),
            rotation: Rotation(Quat::from_rotation_y(angle)),
            state: Default::default(),
        }
    }

    fn moving(
        info_path: AssetPath<'static>,
        cursor_offset: CursorOffset,
        position: Position,
        rotation: Rotation,
    ) -> Self {
        Self {
            object: Object(info_path),
            cursor_offset,
            position,
            rotation,
            state: PlaceState::default(),
        }
    }
}

/// Contains an offset between cursor position on first creation and object origin.
#[derive(Clone, Component, Copy, Default, Deref)]
struct CursorOffset(Vec3);

/// Controls if an object can be placed.
///
/// Stored as a separate component to avoid triggering change detection to update the object material.
#[derive(Component)]
struct PlaceState {
    /// Can be placed without colliding with any other entities.
    collides: bool,

    /// Additional object condition for placing.
    ///
    /// For example, a door can be placed only on a wall. Controlled by other plugins.
    allowed_place: bool,
}

impl PlaceState {
    fn placeable(&self) -> bool {
        !self.collides && self.allowed_place
    }
}

impl Default for PlaceState {
    fn default() -> Self {
        Self {
            allowed_place: true,
            collides: false,
        }
    }
}
