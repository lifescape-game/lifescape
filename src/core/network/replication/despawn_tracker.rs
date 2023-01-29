use bevy::{ecs::system::SystemChangeTick, prelude::*, utils::HashSet};
use bevy_renet::renet::RenetServer;
use iyes_loopless::prelude::*;

use super::{replication_rules::Replication, AckedTicks};

/// Tracks entity despawns of entities with [`Replication`] component in [`DespawnTracker`] resource.
///
/// Used only on server. Despawns will be cleaned after all clients acknowledge them.
pub(super) struct DespawnTrackerPlugin;

impl Plugin for DespawnTrackerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DespawnTracker>()
            .add_system(Self::entity_tracking_system.run_if_resource_exists::<RenetServer>())
            .add_system(Self::cleanup_system.run_if_resource_exists::<RenetServer>())
            .add_system(Self::detection_system.run_if_resource_exists::<RenetServer>());
    }
}

impl DespawnTrackerPlugin {
    fn entity_tracking_system(
        mut tracker: ResMut<DespawnTracker>,
        new_replicated_entities: Query<Entity, Added<Replication>>,
    ) {
        for entity in &new_replicated_entities {
            tracker.tracked_entities.insert(entity);
        }
    }

    /// Cleanups all acknowledged despawns.
    ///
    /// Cleans all despawns if [`AckedTicks`] is empty.
    fn cleanup_system(mut despawn_tracker: ResMut<DespawnTracker>, client_acks: Res<AckedTicks>) {
        despawn_tracker
            .despawns
            .retain(|(_, tick)| client_acks.values().any(|last_tick| last_tick < tick));
    }

    fn detection_system(
        change_tick: SystemChangeTick,
        mut tracker: ResMut<DespawnTracker>,
        entities: Query<Entity>,
    ) {
        let DespawnTracker {
            ref mut tracked_entities,
            ref mut despawns,
        } = *tracker;

        tracked_entities.retain(|&entity| {
            if entities.get(entity).is_err() {
                despawns.push((entity, change_tick.change_tick()));
                false
            } else {
                true
            }
        });
    }
}

#[derive(Default, Resource)]
pub(super) struct DespawnTracker {
    tracked_entities: HashSet<Entity>,
    /// Entities and ticks when they were despawned.
    pub(super) despawns: Vec<(Entity, u32)>,
}

#[cfg(test)]
mod tests {
    use crate::core::network::network_preset::NetworkPresetPlugin;

    use super::*;

    #[test]
    fn detection() {
        let mut app = App::new();
        app.init_resource::<AckedTicks>()
            .add_plugin(NetworkPresetPlugin::server())
            .add_plugin(DespawnTrackerPlugin);

        // To avoid cleanup.
        const DUMMY_CLIENT_ID: u64 = 0;
        app.world
            .resource_mut::<AckedTicks>()
            .insert(DUMMY_CLIENT_ID, 0);

        let replicated_entity = app.world.spawn(Replication).id();

        app.update();

        app.world.entity_mut(replicated_entity).despawn();

        app.update();

        let despawn_tracker = app.world.resource::<DespawnTracker>();
        assert_eq!(despawn_tracker.despawns.len(), 1);
        assert_eq!(
            despawn_tracker.despawns.first().unwrap().0,
            replicated_entity
        );
    }
}
