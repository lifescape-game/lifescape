use bevy::{
    asset::{Asset, AssetPath},
    prelude::*,
};
use strum::IntoEnumIterator;

/// Resource that preload the collection and gives access to it.
#[derive(Resource)]
pub(crate) struct Collection<T: AssetCollection>(Vec<Handle<T::AssetType>>);

impl<T: AssetCollection + IntoEnumIterator> FromWorld for Collection<T> {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        let handles = T::iter()
            .map(|value| value.asset_path())
            .inspect(|asset_path| debug!("preloading '{asset_path}'"))
            .map(|asset_path| asset_server.load(asset_path))
            .collect();

        Self(handles)
    }
}

impl<T: AssetCollection + Into<usize>> Collection<T> {
    pub(crate) fn handle(&self, value: T) -> Handle<T::AssetType> {
        self.0[value.into()].clone()
    }
}

/// Associates type with asset collection.
pub(crate) trait AssetCollection {
    type AssetType: Asset;

    /// Returns associated asset path based on the current value.
    fn asset_path(&self) -> AssetPath<'static>;
}
