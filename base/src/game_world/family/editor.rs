use std::fmt::Write;

use bevy::prelude::*;

use crate::game_world::{
    actor::{human::EditorHuman, SelectedActor},
    family::{FamilyMembers, SelectedFamilyCreated},
    player_camera::PlayerCamera,
    WorldState,
};

pub(crate) struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Self::reset_family)
            .add_observer(Self::show)
            .add_observer(Self::hide)
            .add_systems(OnEnter(WorldState::FamilyEditor), Self::setup)
            .add_systems(
                Update,
                Self::play.run_if(in_state(WorldState::FamilyEditor)),
            )
            .add_systems(
                PostUpdate,
                Self::update_names.run_if(in_state(WorldState::FamilyEditor)),
            );
    }
}

impl EditorPlugin {
    fn setup(mut commands: Commands) {
        debug!("initializing editor");
        commands.spawn(EditorFamily).with_children(|parent| {
            parent.spawn((
                DirectionalLight {
                    shadows_enabled: true,
                    ..Default::default()
                },
                Transform::from_xyz(4.0, 7.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            ));
            parent.spawn(PlayerCamera);
            parent.spawn(EditorSelectedActor);
        });
    }

    fn play(
        mut commands: Commands,
        mut spawn_select_events: EventReader<SelectedFamilyCreated>,
        families: Query<&FamilyMembers>,
    ) {
        for members in families.iter_many(spawn_select_events.read().map(|event| event.0)) {
            info!("starting playing");
            let actor_entity = *members
                .first()
                .expect("family should always have at least one member");
            commands.entity(actor_entity).insert(SelectedActor);
            commands.set_state(WorldState::Family);
        }
    }

    fn reset_family(
        _trigger: Trigger<EditorFamilyReset>,
        mut commands: Commands,
        actors: Query<Entity, With<EditorActor>>,
        family_entity: Single<Entity, With<EditorFamily>>,
    ) {
        info!("resetting family");
        for entity in &actors {
            commands.entity(entity).despawn_recursive();
        }

        // Spawn a new actor for editing.
        commands.entity(*family_entity).with_children(|parent| {
            parent.spawn(EditorSelectedActor);
        });
    }

    fn update_names(
        mut changed_names: Query<
            (Entity, &EditorFirstName, &EditorLastName, &mut Name),
            Or<(Changed<EditorFirstName>, Changed<EditorLastName>)>,
        >,
    ) {
        for (entity, first_name, last_name, mut name) in &mut changed_names {
            debug!("updating full name for `{entity}`");
            name.mutate(|name| {
                name.clear();
                write!(name, "{} {}", first_name.0, last_name.0).unwrap();
            });
        }
    }

    fn show(trigger: Trigger<OnAdd, EditorSelectedActor>, mut actors: Query<&mut Visibility>) {
        debug!("showing `{}`", trigger.entity());
        let mut visibility = actors.get_mut(trigger.entity()).unwrap();
        *visibility = Visibility::Inherited;
    }

    fn hide(trigger: Trigger<OnRemove, EditorSelectedActor>, mut actors: Query<&mut Visibility>) {
        let mut visibility = actors.get_mut(trigger.entity()).unwrap();
        debug!("hiding `{}`", trigger.entity());
        *visibility = Visibility::Hidden;
    }
}

/// A root family editor component.
#[derive(Component, Default)]
#[require(
    Name(|| Name::new("Editor family")),
    Transform,
    Visibility,
    StateScoped::<WorldState>(|| StateScoped(WorldState::FamilyEditor))
)]
pub struct EditorFamily;

/// Component for a actor inside the editor.
#[derive(Component, Default)]
#[require(EditorFirstName, EditorLastName, EditorSex, SceneRoot, EditorHuman)] // TODO: Select race.
pub struct EditorActor;

#[derive(Component, Default, Deref, DerefMut, Clone)]
pub struct EditorFirstName(pub String);

#[derive(Component, Default, Deref, DerefMut, Clone)]
pub struct EditorLastName(pub String);

#[derive(Clone, Copy, Component, Default, Debug, PartialEq)]
pub enum EditorSex {
    #[default]
    Male,
    Female,
}

/// Event that resets currently editing family.
#[derive(Event)]
pub struct EditorFamilyReset;

/// Indicates currently editing actor.
#[derive(Component)]
#[require(EditorActor)]
pub struct EditorSelectedActor;

#[derive(Default, Resource)]
pub struct FamilyScene {
    pub name: String,
    pub actors: Vec<Box<dyn ActorBundle>>,
}

impl FamilyScene {
    pub fn new(name: String) -> Self {
        Self {
            name,
            actors: Default::default(),
        }
    }
}

#[reflect_trait]
pub trait ActorBundle: Reflect {
    #[allow(dead_code)]
    fn glyph(&self) -> &'static str;
}
