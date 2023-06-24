use std::{fs, mem};

use anyhow::{Context, Result};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use derive_more::Display;
use strum::{EnumIter, IntoEnumIterator};

use crate::core::{
    error,
    game_paths::GamePaths,
    game_state::GameState,
    game_world::{GameLoad, GameWorldPlugin, WorldName},
    network::{ConnectionSettings, ServerSettings},
};

use super::{
    theme::Theme,
    widget::{
        button::TextButtonBundle, text_edit::TextEditBundle, ui_root::UiRoot, Dialog, DialogBundle,
        LabelBundle,
    },
};

pub(super) struct WorldBrowserPlugin;

impl Plugin for WorldBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(Self::setup_system.in_schedule(OnEnter(GameState::WorldBrowser)))
            .add_systems(
                (
                    Self::world_button_system.after(GameWorldPlugin::loading_system),
                    Self::host_dialog_button_system
                        .pipe(error::report)
                        .after(GameWorldPlugin::loading_system),
                    Self::remove_dialog_button_system.pipe(error::report),
                    Self::world_browser_button_system,
                    Self::create_dialog_button_system,
                    Self::join_dialog_button_system.pipe(error::report),
                )
                    .in_set(OnUpdate(GameState::WorldBrowser)),
            );
    }
}

impl WorldBrowserPlugin {
    fn setup_system(mut commands: Commands, theme: Res<Theme>, game_paths: Res<GamePaths>) {
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        size: Size::all(Val::Percent(100.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::FlexStart,
                        padding: theme.padding.global,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                UiRoot,
            ))
            .with_children(|parent| {
                parent.spawn(LabelBundle::large(&theme, "World browser"));
                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::all(Val::Percent(100.0)),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::FlexStart,
                            padding: theme.padding.normal,
                            gap: theme.gap.normal,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        let world_names = game_paths
                            .get_world_names()
                            .map_err(|e| error!("unable to get world names: {e}"))
                            .unwrap_or_default();
                        for world_name in world_names {
                            setup_world_node(parent, &theme, world_name);
                        }
                    });

                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::new(Val::Percent(100.0), Val::Auto),
                            justify_content: JustifyContent::FlexStart,
                            gap: theme.gap.normal,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        for button in WorldBrowserButton::iter() {
                            parent.spawn((
                                button,
                                TextButtonBundle::normal(&theme, button.to_string()),
                            ));
                        }
                    });
            });
    }

    fn world_button_system(
        mut commands: Commands,
        mut load_events: EventWriter<GameLoad>,
        theme: Res<Theme>,
        buttons: Query<(&Interaction, &WorldButton, &WorldNode), Changed<Interaction>>,
        mut labels: Query<&mut Text>,
        roots: Query<Entity, With<UiRoot>>,
    ) {
        for (&interaction, world_button, &world_node) in &buttons {
            if interaction != Interaction::Clicked {
                continue;
            }

            let mut world_name = labels
                .get_mut(world_node.label_entity)
                .expect("world label should contain text");
            match world_button {
                WorldButton::Play => {
                    commands
                        .insert_resource(WorldName(mem::take(&mut world_name.sections[0].value)));
                    load_events.send_default();
                }
                WorldButton::Host => setup_host_world_dialog(
                    &mut commands,
                    roots.single(),
                    &theme,
                    world_node,
                    &mut world_name.sections[0].value,
                ),
                WorldButton::Remove => {
                    setup_remove_world_dialog(
                        &mut commands,
                        roots.single(),
                        &theme,
                        world_node,
                        &mut world_name.sections[0].value,
                    );
                }
            }
        }
    }

    fn host_dialog_button_system(
        mut commands: Commands,
        mut load_events: EventWriter<GameLoad>,
        mut server_settings: ResMut<ServerSettings>,
        network_channels: Res<NetworkChannels>,
        dialogs: Query<(Entity, &WorldNode), With<Dialog>>,
        buttons: Query<(&Interaction, &HostDialogButton)>,
        text_edits: Query<&Text, With<PortEdit>>,
        mut labels: Query<&mut Text, Without<PortEdit>>,
    ) -> Result<()> {
        for (&interaction, &button) in &buttons {
            if interaction == Interaction::Clicked {
                let (dialog_entity, world_node) = dialogs.single();
                if button == HostDialogButton::Host {
                    let mut world_name = labels
                        .get_mut(world_node.label_entity)
                        .expect("world label should contain text");
                    commands
                        .insert_resource(WorldName(mem::take(&mut world_name.sections[0].value)));
                    load_events.send_default();

                    // TODO: Maybe remove settings resource.
                    let port = text_edits.single();
                    server_settings.port = port.sections[0].value.parse()?;
                    let (server, transport) = server_settings
                        .create_server(
                            network_channels.server_channels(),
                            network_channels.client_channels(),
                        )
                        .context("unable to create server")?;
                    commands.insert_resource(server);
                    commands.insert_resource(transport);
                }
                commands.entity(dialog_entity).despawn_recursive();
            }
        }

        Ok(())
    }

    fn remove_dialog_button_system(
        mut commands: Commands,
        game_paths: Res<GamePaths>,
        dialogs: Query<(Entity, &WorldNode), With<Dialog>>,
        buttons: Query<(&Interaction, &RemoveDialogButton)>,
        labels: Query<&Text>,
    ) -> Result<()> {
        for (&interaction, &button) in &buttons {
            if interaction == Interaction::Clicked {
                let (dialog_entity, world_node) = dialogs.single();
                let world_name = labels
                    .get(world_node.label_entity)
                    .expect("world label should contain text");
                if button == RemoveDialogButton::Remove {
                    let world_path = game_paths.world_path(&world_name.sections[0].value);
                    fs::remove_file(&world_path)
                        .with_context(|| format!("unable to remove {world_path:?}"))?;
                    commands.entity(world_node.node_entity).despawn_recursive();
                }
                commands.entity(dialog_entity).despawn_recursive();
            }
        }

        Ok(())
    }

    fn world_browser_button_system(
        mut commands: Commands,
        theme: Res<Theme>,
        buttons: Query<(&Interaction, &WorldBrowserButton), Changed<Interaction>>,
        roots: Query<Entity, With<UiRoot>>,
    ) {
        if let Ok((&interaction, button)) = buttons.get_single() {
            if interaction != Interaction::Clicked {
                return;
            }

            match button {
                WorldBrowserButton::Create => {
                    setup_create_world_dialog(&mut commands, roots.single(), &theme)
                }
                WorldBrowserButton::Join => {
                    setup_join_world_dialog(&mut commands, roots.single(), &theme)
                }
            }
        }
    }

    fn create_dialog_button_system(
        mut commands: Commands,
        mut game_state: ResMut<NextState<GameState>>,
        buttons: Query<(&Interaction, &CreateDialogButton), Changed<Interaction>>,
        mut text_edits: Query<&mut Text, With<WorldNameEdit>>,
        dialogs: Query<Entity, With<Dialog>>,
    ) {
        for (&interaction, &button) in &buttons {
            if interaction == Interaction::Clicked {
                if button == CreateDialogButton::Create {
                    let mut world_name = text_edits.single_mut();
                    commands
                        .insert_resource(WorldName(mem::take(&mut world_name.sections[0].value)));
                    game_state.set(GameState::World);
                }
                commands.entity(dialogs.single()).despawn_recursive();
            }
        }
    }

    fn join_dialog_button_system(
        mut commands: Commands,
        mut connection_settings: ResMut<ConnectionSettings>,
        network_channels: Res<NetworkChannels>,
        buttons: Query<(&Interaction, &JoinDialogButton), Changed<Interaction>>,
        port_edits: Query<&Text, With<PortEdit>>,
        mut ip_edits: Query<&mut Text, (With<IpEdit>, Without<PortEdit>)>,
        dialogs: Query<Entity, With<Dialog>>,
    ) -> Result<()> {
        for (&interaction, &button) in &buttons {
            if interaction == Interaction::Clicked {
                match button {
                    JoinDialogButton::Join => {
                        let mut ip = ip_edits.single_mut();
                        let port = port_edits.single();
                        connection_settings.port = port.sections[0].value.parse()?;
                        connection_settings.ip = mem::take(&mut ip.sections[0].value);

                        // TODO: Maybe remove settings resource.
                        let (client, transport) = connection_settings
                            .create_client(
                                network_channels.server_channels(),
                                network_channels.client_channels(),
                            )
                            .context("unable to create connection")?;
                        commands.insert_resource(client);
                        commands.insert_resource(transport);
                    }
                    JoinDialogButton::Cancel => {
                        commands.entity(dialogs.single()).despawn_recursive()
                    }
                }
            }
        }

        Ok(())
    }
}

fn setup_world_node(parent: &mut ChildBuilder, theme: &Theme, label: impl Into<String>) {
    parent
        .spawn(NodeBundle {
            style: Style {
                size: Size::new(Val::Percent(50.0), Val::Percent(30.0)),
                padding: theme.padding.normal,
                ..Default::default()
            },
            background_color: theme.panel_color.into(),
            ..Default::default()
        })
        .with_children(|parent| {
            let node_entity = parent.parent_entity();
            let label_entity = parent.spawn(LabelBundle::large(theme, label)).id();
            parent
                .spawn(NodeBundle {
                    style: Style {
                        size: Size::all(Val::Percent(100.0)),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .add_child(label_entity);
            parent
                .spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        gap: theme.gap.normal,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .with_children(|parent| {
                    for button in WorldButton::iter() {
                        parent.spawn((
                            button,
                            WorldNode {
                                label_entity,
                                node_entity,
                            },
                            TextButtonBundle::normal(theme, button.to_string()),
                        ));
                    }
                });
        });
}

fn setup_host_world_dialog(
    commands: &mut Commands,
    root_entity: Entity,
    theme: &Theme,
    world_node: WorldNode,
    world_name: &str,
) {
    commands.entity(root_entity).with_children(|parent| {
        parent
            .spawn((DialogBundle::new(theme), world_node))
            .with_children(|parent| {
                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::new(Val::Percent(50.0), Val::Percent(25.0)),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            gap: theme.gap.normal,
                            ..Default::default()
                        },
                        background_color: theme.panel_color.into(),
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        parent.spawn(LabelBundle::normal(theme, format!("Host {world_name}")));

                        // TODO: Use or remove world name.
                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    gap: theme.gap.normal,
                                    justify_content: JustifyContent::Center,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                parent.spawn(LabelBundle::normal(theme, "Port:"));
                                parent.spawn((PortEdit, TextEditBundle::empty(theme)));
                            });

                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                for button in HostDialogButton::iter() {
                                    parent.spawn((
                                        button,
                                        TextButtonBundle::normal(theme, button.to_string()),
                                    ));
                                }
                            });
                    });
            });
    });
}

fn setup_remove_world_dialog(
    commands: &mut Commands,
    root_entity: Entity,
    theme: &Theme,
    world_node: WorldNode,
    world_name: &str,
) {
    commands.entity(root_entity).with_children(|parent| {
        parent
            .spawn((DialogBundle::new(theme), world_node))
            .with_children(|parent| {
                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::new(Val::Percent(50.0), Val::Percent(20.0)),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            gap: theme.gap.normal,
                            ..Default::default()
                        },
                        background_color: theme.panel_color.into(),
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        parent.spawn(LabelBundle::normal(
                            theme,
                            format!("Are you sure you want to remove world {world_name}?",),
                        ));

                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                for button in RemoveDialogButton::iter() {
                                    parent.spawn((
                                        button,
                                        TextButtonBundle::normal(theme, button.to_string()),
                                    ));
                                }
                            });
                    });
            });
    });
}

fn setup_create_world_dialog(commands: &mut Commands, root_entity: Entity, theme: &Theme) {
    commands.entity(root_entity).with_children(|parent| {
        parent
            .spawn(DialogBundle::new(&theme))
            .with_children(|parent| {
                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::new(Val::Percent(50.0), Val::Percent(25.0)),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: theme.padding.normal,
                            gap: theme.gap.normal,
                            ..Default::default()
                        },
                        background_color: theme.panel_color.into(),
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        parent.spawn(LabelBundle::normal(&theme, "Create world"));
                        parent.spawn((
                            WorldNameEdit,
                            TextEditBundle::new(&theme, "New world").active(),
                        ));
                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                for button in CreateDialogButton::iter() {
                                    parent.spawn((
                                        button,
                                        TextButtonBundle::normal(&theme, button.to_string()),
                                    ));
                                }
                            });
                    });
            });
    });
}

fn setup_join_world_dialog(commands: &mut Commands, root_entity: Entity, theme: &Theme) {
    commands.entity(root_entity).with_children(|parent| {
        parent
            .spawn(DialogBundle::new(&theme))
            .with_children(|parent| {
                parent
                    .spawn(NodeBundle {
                        style: Style {
                            size: Size::new(Val::Percent(50.0), Val::Percent(30.0)),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            padding: theme.padding.normal,
                            gap: theme.gap.normal,
                            ..Default::default()
                        },
                        background_color: theme.panel_color.into(),
                        ..Default::default()
                    })
                    .with_children(|parent| {
                        parent.spawn(LabelBundle::normal(&theme, "Join world"));

                        // TODO 0.11: Use grid layout
                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                const GRID_GAP: Size = Size::all(Val::Px(10.0));
                                parent
                                    .spawn(NodeBundle {
                                        style: Style {
                                            flex_direction: FlexDirection::Column,
                                            gap: GRID_GAP,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .with_children(|parent| {
                                        parent.spawn(LabelBundle::normal(theme, "IP:"));
                                        parent.spawn(LabelBundle::normal(theme, "Port:"));
                                    });
                                parent
                                    .spawn(NodeBundle {
                                        style: Style {
                                            flex_direction: FlexDirection::Column,
                                            gap: theme.gap.normal,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .with_children(|parent| {
                                        parent.spawn((IpEdit, TextEditBundle::empty(theme)));
                                        parent.spawn((PortEdit, TextEditBundle::empty(theme)));
                                    });
                            });

                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                for button in JoinDialogButton::iter() {
                                    parent.spawn((
                                        button,
                                        TextButtonBundle::normal(theme, button.to_string()),
                                    ));
                                }
                            });
                    });
            });
    });
}

#[derive(Component, EnumIter, Clone, Copy, Display)]
enum WorldButton {
    Play,
    Host,
    Remove,
}

#[derive(Component, EnumIter, Clone, Copy, Display, PartialEq)]
enum RemoveDialogButton {
    Remove,
    Cancel,
}

/// Associated world node entities.
#[derive(Clone, Component, Copy)]
struct WorldNode {
    label_entity: Entity,
    node_entity: Entity,
}

#[derive(Component, EnumIter, Clone, Copy, Display)]
enum WorldBrowserButton {
    Create,
    Join,
}

#[derive(Component, EnumIter, Clone, Copy, Display, PartialEq)]
enum CreateDialogButton {
    Create,
    Cancel,
}

#[derive(Component)]
struct WorldNameEdit;

#[derive(Component)]
struct PortEdit;

#[derive(Component)]
struct IpEdit;

#[derive(Component, EnumIter, Clone, Copy, Display, PartialEq)]
enum HostDialogButton {
    Host,
    Cancel,
}

#[derive(Component, EnumIter, Clone, Copy, Display, PartialEq)]
enum JoinDialogButton {
    Join,
    Cancel,
}
