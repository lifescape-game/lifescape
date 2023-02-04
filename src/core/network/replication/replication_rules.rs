use bevy::{
    prelude::*,
    reflect::GetTypeRegistration,
    utils::{HashMap, HashSet},
};
use bevy_trait_query::imports::{Archetype, ComponentId};

pub(crate) struct ReplicationRulesPlugin;

impl Plugin for ReplicationRulesPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Replication>()
            .init_resource::<ReplicationRules>();
    }
}

pub(crate) trait AppReplicationExt {
    /// A shorthand for [`App::register_type`] with [`Self::replicate`].
    fn register_and_replicate<T: Component + GetTypeRegistration>(&mut self) -> &mut Self;

    /// Marks component for replication.
    ///
    /// The component should be registered, implement [`Reflect`] and have `#[reflect(Component)]`.
    fn replicate<T: Component>(&mut self) -> &mut Self;

    /// Ignores component `T` replication if component `U` is present on the same entity.
    ///
    /// Component `T` should be marked for replication.
    /// Component `U` should be registered.
    /// Could be called multiple times for the same component to disable replication
    /// for different presented components.
    fn not_replicate_if_present<T: Component, U: Component>(&mut self) -> &mut Self;
}

impl AppReplicationExt for App {
    fn register_and_replicate<T: Component + GetTypeRegistration>(&mut self) -> &mut Self {
        self.register_type::<T>().replicate::<T>()
    }

    fn replicate<T: Component>(&mut self) -> &mut Self {
        let component_id = self.world.init_component::<T>();
        let mut replication_rules = self.world.resource_mut::<ReplicationRules>();
        replication_rules.replicated.insert(component_id);
        self
    }

    fn not_replicate_if_present<T: Component, U: Component>(&mut self) -> &mut Self {
        let ignore_id = self.world.init_component::<T>();
        let present_id = self.world.init_component::<U>();
        let mut replication_rules = self.world.resource_mut::<ReplicationRules>();
        replication_rules
            .ignored_if_present
            .entry(ignore_id)
            .or_default()
            .push(present_id);
        self
    }
}

/// Contains [`ComponentId`]'s that used to decide
/// if a component should be replicated.
#[derive(Resource)]
pub(crate) struct ReplicationRules {
    /// Components that should be replicated.
    pub(super) replicated: HashSet<ComponentId>,

    /// Ignore a key component if any of its value components are present in an archetype.
    ignored_if_present: HashMap<ComponentId, Vec<ComponentId>>,

    /// ID of [`Replication`] component, only entities with this components should be replicated.
    replication_id: ComponentId,
}

impl ReplicationRules {
    /// Returns `true` if an entity of an archetype should be replicated.
    pub(crate) fn is_replicated_archetype(&self, archetype: &Archetype) -> bool {
        archetype.contains(self.replication_id)
    }

    /// Returns `true` if a component of an archetype should be replicated.
    pub(crate) fn is_replicated_component(
        &self,
        archetype: &Archetype,
        component_id: ComponentId,
    ) -> bool {
        if self.replicated.contains(&component_id) {
            if let Some(ignore_ids) = self.ignored_if_present.get(&component_id) {
                for &ignore_id in ignore_ids {
                    if archetype.contains(ignore_id) {
                        return false;
                    }
                }
            }
            return true;
        }

        false
    }
}

impl FromWorld for ReplicationRules {
    fn from_world(world: &mut World) -> Self {
        Self {
            replicated: Default::default(),
            ignored_if_present: Default::default(),
            replication_id: world.init_component::<Replication>(),
        }
    }
}

/// Marks entity for replication.
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub(crate) struct Replication;
