pub(crate) mod placing_object;

use std::path::PathBuf;

use bevy::{
    ecs::entity::{EntityMap, MapEntities, MapEntitiesError},
    prelude::*,
};
use bevy_mod_outline::{OutlineBundle, OutlineVolume};
use bevy_rapier3d::prelude::*;
use bevy_renet::renet::RenetClient;
use bevy_scene_hook::SceneHook;
use iyes_loopless::prelude::*;
use placing_object::PlacingObjectPlugin;
use serde::{Deserialize, Serialize};
use tap::TapFallible;

use super::{
    asset_metadata::{self, ObjectMetadata},
    city::{City, CityPlugin},
    collision_groups::DollisGroups,
    game_world::{parent_sync::ParentSync, GameWorld},
    lot::LotVertices,
    network::{
        network_event::{
            client_event::{ClientEvent, ClientEventAppExt},
            server_event::{SendMode, ServerEvent, ServerEventAppExt},
        },
        replication::replication_rules::{AppReplicationExt, Replication},
    },
    picking::Pickable,
};

pub(super) struct ObjectPlugin;

impl Plugin for ObjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(PlacingObjectPlugin)
            .register_and_replicate::<ObjectPath>()
            .add_client_event::<ObjectSpawn>()
            .add_mapped_client_event::<ObjectMove>()
            .add_mapped_client_event::<ObjectDespawn>()
            .add_server_event::<ObjectEventConfirmed>()
            .add_system(Self::init_system.run_if_resource_exists::<GameWorld>())
            .add_system(Self::spawn_system.run_unless_resource_exists::<RenetClient>())
            .add_system(Self::movement_system.run_unless_resource_exists::<RenetClient>())
            .add_system(Self::despawn_system.run_unless_resource_exists::<RenetClient>());
    }
}

impl ObjectPlugin {
    fn init_system(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        object_metadata: Res<Assets<ObjectMetadata>>,
        spawned_objects: Query<(Entity, &ObjectPath), Added<ObjectPath>>,
    ) {
        for (entity, object_path) in &spawned_objects {
            let metadata_handle = asset_server.load(&*object_path.0);
            let object_metadata = object_metadata
                .get(&metadata_handle)
                .unwrap_or_else(|| panic!("object metadata {:?} is invalid", object_path.0));

            let scene_path = asset_metadata::scene_path(&object_path.0);
            let scene_handle: Handle<Scene> = asset_server.load(&scene_path);

            commands.entity(entity).insert((
                scene_handle,
                Name::new(object_metadata.general.name.clone()),
                Pickable,
                AsyncSceneCollider::default(),
                GlobalTransform::default(),
                VisibilityBundle::default(),
                SceneHook::new(|entity, commands| {
                    if entity.contains::<Handle<Mesh>>() {
                        commands.insert((
                            CollisionGroups::new(Group::OBJECT, Group::ALL),
                            OutlineBundle {
                                outline: OutlineVolume {
                                    visible: false,
                                    colour: Color::rgba(1.0, 1.0, 1.0, 0.3),
                                    width: 2.0,
                                },
                                ..Default::default()
                            },
                        ));
                    }
                }),
            ));
            debug!("spawned object {scene_path:?}");
        }
    }

    fn spawn_system(
        mut commands: Commands,
        mut spawn_events: EventReader<ClientEvent<ObjectSpawn>>,
        mut confirm_events: EventWriter<ServerEvent<ObjectEventConfirmed>>,
        cities: Query<(Entity, &Transform), With<City>>,
        lots: Query<(Entity, &LotVertices)>,
    ) {
        for ClientEvent { client_id, event } in spawn_events.iter().cloned() {
            const HALF_CITY_SIZE: f32 = CityPlugin::CITY_SIZE / 2.0;
            if event.position.y.abs() > HALF_CITY_SIZE {
                error!(
                    "received object spawn position {} with 'y' outside of city size",
                    event.position
                );
                continue;
            }

            let Some(city_entity) = cities
                .iter()
                .map(|(entity, transform)| (entity, transform.translation.x - event.position.x))
                .find(|(_, x)| x.abs() < HALF_CITY_SIZE)
                .map(|(entity, _)| entity)
            else {
                error!("unable to find a city for object spawn position {}", event.position);
                continue;
            };

            // TODO: Add a check if user can spawn an object on the lot.
            let parent_entity = lots
                .iter()
                .find(|(_, vertices)| vertices.contains_point(event.position))
                .map(|(lot_entity, _)| lot_entity)
                .unwrap_or(city_entity);

            commands.spawn(ObjectBundle::new(
                event.metadata_path,
                Vec3::new(event.position.x, 0.0, event.position.y),
                event.rotation,
                parent_entity,
            ));
            confirm_events.send(ServerEvent {
                mode: SendMode::Direct(client_id),
                event: ObjectEventConfirmed,
            });
        }
    }

    fn movement_system(
        mut move_events: EventReader<ClientEvent<ObjectMove>>,
        mut confirm_events: EventWriter<ServerEvent<ObjectEventConfirmed>>,
        mut transforms: Query<&mut Transform>,
    ) {
        for ClientEvent { client_id, event } in move_events.iter().copied() {
            if let Ok(mut transform) = transforms
                .get_mut(event.entity)
                .tap_err(|e| error!("unable to apply movement from client {client_id}: {e}"))
            {
                transform.translation = event.translation;
                transform.rotation = event.rotation;
                confirm_events.send(ServerEvent {
                    mode: SendMode::Direct(client_id),
                    event: ObjectEventConfirmed,
                });
            }
        }
    }

    fn despawn_system(
        mut commands: Commands,
        mut despawn_events: EventReader<ClientEvent<ObjectDespawn>>,
        mut confirm_events: EventWriter<ServerEvent<ObjectEventConfirmed>>,
    ) {
        for ClientEvent { client_id, event } in despawn_events.iter().copied() {
            commands.entity(event.0).despawn_recursive();
            confirm_events.send(ServerEvent {
                mode: SendMode::Direct(client_id),
                event: ObjectEventConfirmed,
            });
        }
    }
}

#[derive(Bundle)]
struct ObjectBundle {
    object_path: ObjectPath,
    transform: Transform,
    parent_sync: ParentSync,
    replication: Replication,
}

impl ObjectBundle {
    fn new(metadata_path: PathBuf, translation: Vec3, rotation: Quat, parent: Entity) -> Self {
        Self {
            object_path: ObjectPath(metadata_path),
            transform: Transform::default()
                .with_translation(translation)
                .with_rotation(rotation),
            parent_sync: ParentSync(parent),
            replication: Replication,
        }
    }
}

#[derive(Clone, Component, Debug, Default, Reflect)]
#[reflect(Component)]
pub(crate) struct ObjectPath(PathBuf);

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ObjectSpawn {
    metadata_path: PathBuf,
    position: Vec2,
    rotation: Quat,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct ObjectMove {
    entity: Entity,
    translation: Vec3,
    rotation: Quat,
}

impl MapEntities for ObjectMove {
    fn map_entities(&mut self, entity_map: &EntityMap) -> Result<(), MapEntitiesError> {
        self.entity = entity_map.get(self.entity)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct ObjectDespawn(Entity);

impl MapEntities for ObjectDespawn {
    fn map_entities(&mut self, entity_map: &EntityMap) -> Result<(), MapEntitiesError> {
        self.0 = entity_map.get(self.0)?;
        Ok(())
    }
}

/// An event from server which indicates action confirmation.
#[derive(Deserialize, Serialize, Debug, Default)]
struct ObjectEventConfirmed;
