use bevy::{
    ecs::{
        entity::{EntityMap, MapEntities, MapEntitiesError},
        reflect::ReflectMapEntities,
    },
    prelude::*,
};
use iyes_loopless::prelude::*;

use super::GameWorld;
use crate::core::network::replication::{
    map_entity::ReflectMapEntity, replication_rules::AppReplicationExt,
};

pub(super) struct ParentSyncPlugin;

/// Automatically updates hierarchy when [`SyncParent`] is changed.
///
/// This allows to save / replicate hierarchy using only [`SyncParent`] component.
impl Plugin for ParentSyncPlugin {
    fn build(&self, app: &mut App) {
        app.register_and_replicate::<ParentSync>()
            .add_system(Self::parent_sync_system.run_if_resource_exists::<GameWorld>());
    }
}

impl ParentSyncPlugin {
    fn parent_sync_system(
        mut commands: Commands,
        changed_parents: Query<(Entity, &ParentSync), Changed<ParentSync>>,
    ) {
        for (entity, parent) in &changed_parents {
            commands.entity(parent.0).push_children(&[entity]);
        }
    }
}

#[derive(Component, Reflect, Clone, Copy)]
#[reflect(Component, MapEntities, MapEntity)]
pub(crate) struct ParentSync(pub(crate) Entity);

// We need to impl either [`FromWorld`] or [`Default`] so [`SyncParent`] can be registered as [`Reflect`].
// Same technicue is used in Bevy for [`Parent`]
impl FromWorld for ParentSync {
    fn from_world(_world: &mut World) -> Self {
        Self(Entity::from_raw(u32::MAX))
    }
}

impl MapEntities for ParentSync {
    fn map_entities(&mut self, entity_map: &EntityMap) -> Result<(), MapEntitiesError> {
        self.0 = entity_map.get(self.0)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bevy::{asset::AssetPlugin, scene::ScenePlugin};

    use super::*;
    use crate::core::network::replication::replication_rules::ReplicationRulesPlugin;

    #[test]
    fn entity_mapping() {
        let mut app = App::new();
        app.init_resource::<GameWorld>()
            .add_plugin(AssetPlugin::default())
            .add_plugin(ScenePlugin)
            .add_plugin(ReplicationRulesPlugin)
            .add_plugin(ParentSyncPlugin);

        let mut other_world = World::new();
        let parent = other_world.spawn_empty().id();
        other_world.spawn(ParentSync(parent));
        let dynamic_scene =
            DynamicScene::from_world(&other_world, app.world.resource::<AppTypeRegistry>());

        let mut scenes = app.world.resource_mut::<Assets<DynamicScene>>();
        let scene_handle = scenes.add(dynamic_scene);
        let mut scene_spawner = app.world.resource_mut::<SceneSpawner>();
        scene_spawner.spawn_dynamic(scene_handle);

        app.update();

        let (child_parent, parent_sync) = app
            .world
            .query::<(&Parent, &ParentSync)>()
            .single(&app.world);
        assert_eq!(child_parent.get(), parent_sync.0);
    }
}
