use std::f32::consts::PI;

use bevy::{
    asset::LoadState,
    prelude::*,
    render::{
        camera::RenderTarget,
        render_resource::{Extent3d, TextureUsages},
        view::{NoFrustumCulling, RenderLayers},
    },
};

use crate::core::asset::metadata::{self, object_metadata::ObjectMetadata};

pub(super) struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<PreviewState>()
            .add_systems(Startup, Self::spawn_camera_system)
            .add_systems(OnEnter(PreviewState::Inactive), Self::deactivation_system)
            .add_systems(
                Update,
                (
                    Self::scene_spawning_system.run_if(in_state(PreviewState::Inactive)),
                    Self::loading_system.run_if(in_state(PreviewState::LoadingAsset)),
                    Self::rendering_system.run_if(in_state(PreviewState::Rendering)),
                ),
            );
    }
}

impl PreviewPlugin {
    fn spawn_camera_system(mut commands: Commands) {
        commands.spawn(PreviewCameraBundle::default());
    }

    fn scene_spawning_system(
        mut commands: Commands,
        mut preview_state: ResMut<NextState<PreviewState>>,
        asset_server: Res<AssetServer>,
        object_metadata: Res<Assets<ObjectMetadata>>,
        previews: Query<
            (Entity, &Preview, Option<&Handle<ObjectMetadata>>),
            Without<PreviewProcessed>,
        >,
        parents: Query<&Parent>,
        styles: Query<&Style>,
        actors: Query<&Handle<Scene>>,
        preview_cameras: Query<Entity, With<PreviewCamera>>,
    ) {
        if let Some((preview_entity, preview, metadata_handle)) =
            previews.iter().find(|&(entity, ..)| {
                styles
                    .iter_many(parents.iter_ancestors(entity))
                    .all(|style| style.display != Display::None)
            })
        {
            let (translation, scene_handle) = match preview.kind {
                PreviewKind::Actor(entity) => {
                    debug!("generating preview for actor {entity:?}");

                    let scene_handle = actors
                        .get(entity)
                        .expect("actor for preview should have a scene handle");

                    (Vec3::new(0.0, -1.67, -0.42), scene_handle.clone())
                }
                PreviewKind::Object => {
                    let metadata_handle = metadata_handle
                        .expect("metadata handle component should be present for object previews");
                    let metadata_path = metadata_handle.path().unwrap();
                    debug!("generating preview for object {metadata_path:?}");

                    let metadata = object_metadata.get(metadata_handle).unwrap();
                    let scene_handle = asset_server.load(metadata::scene_path(metadata_path));

                    (metadata.general.preview_translation, scene_handle)
                }
            };

            commands.entity(preview_entity).insert(PreviewProcessed);
            commands
                .entity(preview_cameras.single())
                .with_children(|parent| {
                    parent.spawn(PreviewSceneBundle::new(
                        translation,
                        scene_handle,
                        preview_entity,
                    ));
                });

            preview_state.set(PreviewState::LoadingAsset);
        }
    }

    fn loading_system(
        mut commands: Commands,
        mut asset_events: EventWriter<AssetEvent<Image>>,
        mut preview_state: ResMut<NextState<PreviewState>>,
        mut images: ResMut<Assets<Image>>,
        asset_server: Res<AssetServer>,
        mut preview_cameras: Query<&mut Camera, With<PreviewCamera>>,
        preview_scenes: Query<(&PreviewTarget, &Handle<Scene>)>,
        previews: Query<&Preview>,
    ) {
        let (preview_target, scene_handle) = preview_scenes.single();
        match asset_server.get_load_state(scene_handle).unwrap() {
            LoadState::NotLoaded | LoadState::Loading => (),
            LoadState::Loaded => {
                debug!("asset for preview was sucessfully loaded");

                let Ok(preview) = previews.get(preview_target.0) else {
                    // Entity target is longer valid.
                    preview_state.set(PreviewState::Inactive);
                    return;
                };

                let mut image = Image::default();
                image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
                image.resize(Extent3d {
                    width: preview.width,
                    height: preview.height,
                    ..Default::default()
                });

                let image_handle = images.add(image);
                commands
                    .entity(preview_target.0)
                    .insert(image_handle.clone());

                // A workaround for this bug: https://github.com/bevyengine/bevy/issues/5595.
                asset_events.send(AssetEvent::Modified {
                    id: image_handle.id(),
                });

                let mut camera = preview_cameras.single_mut();
                camera.is_active = true;
                camera.target = RenderTarget::Image(image_handle);

                preview_state.set(PreviewState::Rendering);
            }
            LoadState::Failed => {
                error!("unable to load asset for preview");

                preview_state.set(PreviewState::Inactive);
            }
        }
    }

    fn rendering_system(
        mut commands: Commands,
        mut preview_state: ResMut<NextState<PreviewState>>,
        preview_scenes: Query<Entity, With<PreviewTarget>>,
        chidlren: Query<&Children>,
        meshes: Query<(), With<Handle<Mesh>>>,
    ) {
        for child_entity in chidlren
            .iter_descendants(preview_scenes.single())
            .filter(|&entity| meshes.get(entity).is_ok())
        {
            commands
                .entity(child_entity)
                .insert((PREVIEW_RENDER_LAYER, NoFrustumCulling));
        }

        preview_state.set(PreviewState::Inactive);
        debug!("rendering preview");
    }

    fn deactivation_system(
        mut commands: Commands,
        mut preview_cameras: Query<&mut Camera, With<PreviewCamera>>,
        preview_scenes: Query<Entity, With<PreviewTarget>>,
    ) {
        if let Ok(entity) = preview_scenes.get_single() {
            commands.entity(entity).despawn_recursive();
        }
        preview_cameras.single_mut().is_active = false;
    }
}

const PREVIEW_RENDER_LAYER: RenderLayers = RenderLayers::layer(1);

#[derive(Bundle)]
struct PreviewCameraBundle {
    name: Name,
    preview_camera: PreviewCamera,
    render_layer: RenderLayers,
    ui_config: UiCameraConfig,
    camera_bundle: Camera3dBundle,
    visibility_bundle: VisibilityBundle,
}

impl Default for PreviewCameraBundle {
    fn default() -> Self {
        Self {
            name: "Preview camera".into(),
            preview_camera: PreviewCamera,
            render_layer: PREVIEW_RENDER_LAYER,
            camera_bundle: Camera3dBundle {
                transform: Transform::from_translation(Vec3::Y * 1000.0), // High above the player to avoid noticing.
                camera: Camera {
                    hdr: true,
                    is_active: false,
                    ..Default::default()
                },
                ..Default::default()
            },
            ui_config: UiCameraConfig { show_ui: false },
            // Preview scenes will be spawned as children so this component is necessary in order to have scenes visible.
            visibility_bundle: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, States)]
enum PreviewState {
    #[default]
    Inactive,
    LoadingAsset,
    Rendering,
}

/// Marker for preview camera.
#[derive(Component)]
struct PreviewCamera;

/// Contains information about the preview, generated image handle will be added as a child.
///
/// Preview generation happens only if UI element entity is visible.
/// Processed entities will be marked with [`PreviewProcessed`].
#[derive(Component)]
pub(crate) struct Preview {
    kind: PreviewKind,
    width: u32,
    height: u32,
}

impl Preview {
    pub(crate) fn object(style: &Style) -> Self {
        Self::new(PreviewKind::Object, style)
    }

    pub(crate) fn actor(entity: Entity, style: &Style) -> Self {
        Self::new(PreviewKind::Actor(entity), style)
    }

    fn new(kind: PreviewKind, style: &Style) -> Self {
        let (Val::Px(width), Val::Px(height)) = (style.width, style.height) else {
            panic!("button size should be set in pixels");
        };

        Self {
            kind,
            width: width as u32,
            height: height as u32,
        }
    }
}

/// Specifies where preview should be generated for specific actor in the world or for an object by its metadata.
enum PreviewKind {
    /// Actor entity.
    Actor(Entity),
    /// An object whose metadata handle is a component of the entity.
    Object,
}

/// Marks entity with [`Preview`] as processed end excludes it from preview generation.
#[derive(Component)]
pub(super) struct PreviewProcessed;

/// Scene that used for preview generation.
#[derive(Bundle)]
struct PreviewSceneBundle {
    name: Name,
    preview_target: PreviewTarget,
    scene_bundle: SceneBundle,
}

impl PreviewSceneBundle {
    fn new(translation: Vec3, scene_handle: Handle<Scene>, preview_entity: Entity) -> Self {
        Self {
            name: "Preview scene".into(),
            preview_target: PreviewTarget(preview_entity),
            scene_bundle: SceneBundle {
                scene: scene_handle,
                transform: Transform::from_translation(translation)
                    .with_rotation(Quat::from_rotation_y(PI)), // Rotate towards camera.
                ..Default::default()
            },
        }
    }
}

/// Points to the entity for which the preview will be generated.
#[derive(Component)]
struct PreviewTarget(Entity);
