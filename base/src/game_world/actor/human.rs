use std::mem;

use bevy::{asset::AssetPath, ecs::reflect::ReflectBundle, prelude::*};
use bevy_replicon::prelude::*;
use num_enum::IntoPrimitive;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use super::{
    needs::{Bladder, Energy, Fun, Hunger, Hygiene, Need, NeedBundle, Social},
    Actor, ActorBundle, FirstName, LastName, ReflectActorBundle, Sex,
};
use crate::{
    asset::collection::{AssetCollection, Collection},
    core::GameState,
    game_world::{
        family::{editor::EditableActor, FamilyScene},
        WorldState,
    },
};

pub(super) struct HumanPlugin;

impl Plugin for HumanPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Human>()
            .replicate::<Human>()
            .register_type::<HumanBundle>()
            .init_resource::<Collection<HumanScene>>()
            .add_systems(
                PreUpdate,
                Self::update_sex
                    .after(ClientSet::Receive)
                    .run_if(in_state(GameState::InGame)),
            )
            .add_systems(
                Update,
                (
                    Self::init_needs, // Should run after `ParentSync` to detect if needs was initialized correctly.
                    Self::init_children.run_if(in_state(WorldState::FamilyEditor)),
                ),
            );
    }
}

impl HumanPlugin {
    fn init_needs(
        mut commands: Commands,
        actors: Query<(Entity, Option<&Children>), (Added<Human>, With<Actor>)>,
        need: Query<(), With<Need>>,
    ) {
        for (entity, children) in &actors {
            if need
                .iter_many(children.into_iter().flatten())
                .next()
                .is_none()
            {
                debug!("initializing human `{entity}`");
                commands.entity(entity).with_children(|parent| {
                    parent.spawn(NeedBundle::<Bladder>::default());
                    parent.spawn(NeedBundle::<Energy>::default());
                    parent.spawn(NeedBundle::<Fun>::default());
                    parent.spawn(NeedBundle::<Hunger>::default());
                    parent.spawn(NeedBundle::<Hygiene>::default());
                    parent.spawn(NeedBundle::<Social>::default());
                });
            }
        }
    }

    fn update_sex(
        mut commands: Commands,
        human_scenes: Res<Collection<HumanScene>>,
        actors: Query<(Entity, &Sex), (Changed<Sex>, With<Human>)>,
    ) {
        for (entity, &sex) in &actors {
            debug!("initializing sex for human `{entity}`");
            commands
                .entity(entity)
                .insert(human_scenes.handle(sex.into()));
        }
    }

    /// Fills [`FamilyScene`] with editing human actors.
    fn init_children(
        mut family_scenes: Query<&mut FamilyScene, Added<FamilyScene>>,
        mut actors: Query<(&mut FirstName, &mut LastName, &Sex), With<EditableActor>>,
    ) {
        if let Ok(mut family_scene) = family_scenes.get_single_mut() {
            for (mut first_name, mut last_name, &sex) in &mut actors {
                debug!(
                    "adding human '{} {}' to family scene '{}'",
                    first_name.0, last_name.0, family_scene.name
                );
                family_scene.actors.push(Box::new(HumanBundle::new(
                    mem::take(&mut first_name),
                    mem::take(&mut last_name),
                    sex,
                )));
            }
        }
    }
}

#[derive(Component, Default, Deserialize, Reflect, Serialize)]
#[reflect(Component)]
pub(crate) struct Human;

#[derive(Bundle, Default, Reflect)]
#[reflect(Bundle, ActorBundle)]
struct HumanBundle {
    first_name: FirstName,
    last_name: LastName,
    sex: Sex,
    human: Human,
}

impl HumanBundle {
    fn new(first_name: FirstName, last_name: LastName, sex: Sex) -> Self {
        Self {
            first_name,
            last_name,
            sex,
            human: Human,
        }
    }
}

impl ActorBundle for HumanBundle {
    fn glyph(&self) -> &'static str {
        "👤"
    }
}

#[derive(Clone, Copy, IntoPrimitive, EnumIter, Default)]
#[repr(usize)]
enum HumanScene {
    #[default]
    Male,
    Female,
}

impl AssetCollection for HumanScene {
    type AssetType = Scene;

    fn asset_path(&self) -> AssetPath<'static> {
        match self {
            Self::Male => GltfAssetLabel::Scene(0).from_asset("base/actors/bot/y_bot/y_bot.gltf"),
            Self::Female => GltfAssetLabel::Scene(0).from_asset("base/actors/bot/x_bot/x_bot.gltf"),
        }
    }
}

impl From<Sex> for HumanScene {
    fn from(value: Sex) -> Self {
        match value {
            Sex::Male => Self::Male,
            Sex::Female => Self::Female,
        }
    }
}
