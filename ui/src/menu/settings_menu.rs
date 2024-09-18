use bevy::{prelude::*, reflect::GetPath, ui::FocusPolicy};
use leafwing_input_manager::user_input::InputKind;
use strum::{Display, EnumIter, IntoEnumIterator};

use project_harmonia_base::{
    input_events::InputEvents,
    settings::{Action, Settings, SettingsApply},
};
use project_harmonia_widgets::{
    button::{ButtonText, ExclusiveButton, TabContent, TextButtonBundle, Toggled},
    checkbox::{Checkbox, CheckboxBundle},
    click::Click,
    dialog::DialogBundle,
    label::LabelBundle,
    theme::Theme,
};

pub(super) struct SettingsMenuPlugin;

impl Plugin for SettingsMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SettingsMenuOpen>()
            .add_systems(
                Update,
                (
                    Self::update_mapping_text,
                    Self::start_mapping,
                    Self::read_binding,
                    Self::handle_binding_dialog_clicks,
                    Self::handle_settings_menu_clicks,
                )
                    .run_if(any_with_component::<SettingsMenu>),
            )
            .add_systems(
                PostUpdate,
                Self::setup.run_if(on_event::<SettingsMenuOpen>()),
            );
    }
}

impl SettingsMenuPlugin {
    fn setup(
        mut commands: Commands,
        mut tab_commands: Commands,
        settings: Res<Settings>,
        theme: Res<Theme>,
        roots: Query<Entity, (With<Node>, Without<Parent>)>,
    ) {
        info!("opening setting menu");
        commands.entity(roots.single()).with_children(|parent| {
            parent
                .spawn((
                    SettingsMenu,
                    Interaction::None,
                    NodeBundle {
                        style: Style {
                            position_type: PositionType::Absolute,
                            flex_direction: FlexDirection::Column,
                            align_self: AlignSelf::Center,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            padding: theme.padding.global,
                            ..Default::default()
                        },
                        focus_policy: FocusPolicy::Block,
                        background_color: theme.background_color.into(),
                        ..Default::default()
                    },
                ))
                .with_children(|parent| {
                    let tabs_entity = parent
                        .spawn(NodeBundle {
                            style: Style {
                                justify_content: JustifyContent::Center,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .id();

                    for tab in SettingsTab::iter() {
                        let content_entity = parent
                            .spawn(NodeBundle {
                                style: Style {
                                    padding: theme.padding.normal,
                                    column_gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .with_children(|parent| match tab {
                                SettingsTab::Video => setup_video_tab(parent, &theme, &settings),
                                SettingsTab::Controls => {
                                    setup_controls_tab(parent, &theme, &settings)
                                }
                                SettingsTab::Developer => {
                                    setup_developer_tab(parent, &theme, &settings)
                                }
                            })
                            .id();

                        tab_commands
                            .spawn((
                                TabContent(content_entity),
                                ExclusiveButton,
                                Toggled(tab == Default::default()),
                                TextButtonBundle::normal(&theme, tab.to_string()),
                            ))
                            .set_parent(tabs_entity);
                    }

                    parent
                        .spawn(NodeBundle {
                            style: Style {
                                align_items: AlignItems::End,
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                justify_content: JustifyContent::End,
                                column_gap: theme.gap.normal,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .with_children(|parent| {
                            for button in SettingsButton::iter() {
                                parent.spawn((
                                    button,
                                    TextButtonBundle::normal(&theme, button.to_string()),
                                ));
                            }
                        });
                });
        });
    }

    fn update_mapping_text(mut buttons: Query<(&Mapping, &mut ButtonText), Changed<Mapping>>) {
        for (mapping, mut text) in &mut buttons {
            text.0 = match mapping.input_kind {
                Some(InputKind::GamepadButton(gamepad_button)) => {
                    format!("{gamepad_button:?}")
                }
                Some(InputKind::PhysicalKey(keycode)) => {
                    format!("{keycode:?}")
                }
                Some(InputKind::Mouse(mouse_button)) => {
                    format!("{mouse_button:?}")
                }
                _ => "Empty".to_string(),
            };
        }
    }

    fn start_mapping(
        mut commands: Commands,
        mut click_events: EventReader<Click>,
        theme: Res<Theme>,
        roots: Query<Entity, (With<Node>, Without<Parent>)>,
        buttons: Query<(Entity, &Mapping)>,
    ) {
        for (entity, mapping) in buttons.iter_many(click_events.read().map(|event| event.0)) {
            info!("starting binding for '{}'", mapping.action);
            commands.entity(roots.single()).with_children(|parent| {
                parent
                    .spawn((BindingButton(entity), DialogBundle::new(&theme)))
                    .with_children(|parent| {
                        parent
                            .spawn(NodeBundle {
                                style: Style {
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: theme.padding.normal,
                                    row_gap: theme.gap.normal,
                                    ..Default::default()
                                },
                                background_color: theme.panel_color.into(),
                                ..Default::default()
                            })
                            .with_children(|parent| {
                                parent.spawn((
                                    BindingLabel,
                                    LabelBundle::normal(
                                        &theme,
                                        format!("Binding \"{}\", press any key", mapping.action),
                                    ),
                                ));
                                parent
                                    .spawn(NodeBundle {
                                        style: Style {
                                            column_gap: theme.gap.normal,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .with_children(|parent| {
                                        for button in BindingDialogButton::iter() {
                                            // Replace is hidden by default and will be
                                            // displayed only in case of binding conflict.
                                            let display = if button == BindingDialogButton::Replace
                                            {
                                                Display::None
                                            } else {
                                                Default::default()
                                            };

                                            parent.spawn((
                                                button,
                                                TextButtonBundle::normal(
                                                    &theme,
                                                    button.to_string(),
                                                )
                                                .with_display(display),
                                            ));
                                        }
                                    });
                            });
                    });
            });
        }
    }

    fn read_binding(
        mut commands: Commands,
        mut input_events: InputEvents,
        dialogs: Query<(Entity, &BindingButton)>,
        mut mapping_buttons: Query<(Entity, &mut Mapping)>,
        mut labels: Query<&mut Text, With<BindingLabel>>,
        mut dialog_buttons: Query<(&mut Style, &BindingDialogButton)>,
    ) {
        let Ok((dialog_entity, binding_button)) = dialogs.get_single() else {
            return;
        };

        let Some(input_kind) = input_events.input_kind() else {
            return;
        };

        if let Some((conflict_entity, mapping)) = mapping_buttons
            .iter()
            .find(|(_, mapping)| mapping.input_kind == Some(input_kind))
        {
            info!("found conflict with '{}'", mapping.action);
            labels.single_mut().sections[0].value = format!(
                "\"{input_kind}\" is already used by \"{:?}\"",
                mapping.action
            );

            commands
                .entity(dialog_entity)
                .insert(ConflictButton(conflict_entity));

            let (mut style, _) = dialog_buttons
                .iter_mut()
                .find(|(_, &button)| button == BindingDialogButton::Replace)
                .expect("replace button should be spawned with the dialog");
            style.display = Display::Flex;
        } else {
            let (_, mut mapping) = mapping_buttons
                .get_mut(binding_button.0)
                .expect("binding dialog should point to a button with mapping");
            mapping.input_kind = Some(input_kind);
            info!("assigning binding to '{}'", mapping.action);
            commands.entity(dialog_entity).despawn_recursive();
        }
    }

    fn handle_binding_dialog_clicks(
        mut commands: Commands,
        mut click_events: EventReader<Click>,
        mut mapping_buttons: Query<&mut Mapping>,
        dialog_buttons: Query<&BindingDialogButton>,
        dialogs: Query<(Entity, Option<&ConflictButton>, &BindingButton)>,
    ) {
        for dialog_button in dialog_buttons.iter_many(click_events.read().map(|event| event.0)) {
            let (entity, conflict_button, binding_button) = dialogs.single();
            match dialog_button {
                BindingDialogButton::Replace => {
                    let conflict_button = conflict_button
                        .expect("replace button should be clickable only with conflict");
                    let mut conflict_mapping = mapping_buttons
                        .get_mut(conflict_button.0)
                        .expect("binding conflict should point to a button");
                    let input_kind = conflict_mapping.input_kind;
                    conflict_mapping.input_kind = None;

                    let mut mapping = mapping_buttons
                        .get_mut(binding_button.0)
                        .expect("binding should point to a button");
                    mapping.input_kind = input_kind;
                    info!("reassigning binding to '{}'", mapping.action);
                }
                BindingDialogButton::Delete => {
                    let mut mapping = mapping_buttons
                        .get_mut(binding_button.0)
                        .expect("binding should point to a button");
                    info!("deleting binding for '{}'", mapping.action);
                    mapping.input_kind = None;
                }
                BindingDialogButton::Cancel => info!("cancelling binding"),
            }
            commands.entity(entity).despawn_recursive();
        }
    }

    fn handle_settings_menu_clicks(
        mut commands: Commands,
        mut apply_events: EventWriter<SettingsApply>,
        mut click_events: EventReader<Click>,
        mut settings: ResMut<Settings>,
        settings_menus: Query<Entity, With<SettingsMenu>>,
        settings_buttons: Query<&SettingsButton>,
        mapping_buttons: Query<&Mapping>,
        checkboxes: Query<(&Checkbox, &SettingsField)>,
    ) {
        for &settings_button in settings_buttons.iter_many(click_events.read().map(|event| event.0))
        {
            if settings_button == SettingsButton::Ok {
                for (checkbox, field) in &checkboxes {
                    let field_value = settings
                        .path_mut::<bool>(field.0)
                        .expect("fields with checkboxes should be stored as bools");
                    *field_value = checkbox.0;
                }
                settings.controls.mappings.clear();
                for mapping in &mapping_buttons {
                    if let Some(input_kind) = mapping.input_kind {
                        settings
                            .controls
                            .mappings
                            .entry(mapping.action)
                            .or_default()
                            .push(input_kind);
                    }
                }
                apply_events.send_default();
            }

            info!("closing settings menu");
            commands.entity(settings_menus.single()).despawn_recursive()
        }
    }
}

/// Creates [`SettingsField`] from passed field.
macro_rules! setting_field {
    ($path:expr) => {{
        let _validate_field = $path;
        SettingsField(stringify!($path).split_once('.').unwrap().1)
    }};
}

fn setup_video_tab(parent: &mut ChildBuilder, theme: &Theme, settings: &Settings) {
    parent
        .spawn(NodeBundle {
            style: Style {
                flex_direction: FlexDirection::Column,
                row_gap: theme.gap.normal,
                ..Default::default()
            },
            ..Default::default()
        })
        .with_children(|parent| {
            parent.spawn((
                CheckboxBundle::new(theme, settings.video.fullscreen, "Fullscreen"),
                setting_field!(settings.video.fullscreen),
            ));
        });
}

fn setup_controls_tab(parent: &mut ChildBuilder, theme: &Theme, settings: &Settings) {
    const INPUTS_PER_ACTION: usize = 3;
    parent
        .spawn(NodeBundle {
            style: Style {
                display: Display::Grid,
                column_gap: theme.gap.normal,
                row_gap: theme.gap.normal,
                grid_template_columns: vec![GridTrack::auto(); INPUTS_PER_ACTION + 1],
                ..Default::default()
            },
            ..Default::default()
        })
        .with_children(|parent| {
            for (&action, inputs) in &settings.controls.mappings {
                parent.spawn(TextBundle::from_section(
                    action.to_string(),
                    theme.label.normal.clone(),
                ));

                for index in 0..INPUTS_PER_ACTION {
                    parent.spawn((
                        Mapping {
                            action,
                            input_kind: inputs.get(index).cloned(),
                        },
                        TextButtonBundle::normal(theme, String::new()),
                    ));
                }
            }
        });
}

fn setup_developer_tab(parent: &mut ChildBuilder, theme: &Theme, settings: &Settings) {
    parent
        .spawn(NodeBundle {
            style: Style {
                flex_direction: FlexDirection::Column,
                row_gap: theme.gap.normal,
                ..Default::default()
            },
            ..Default::default()
        })
        .with_children(|parent| {
            parent.spawn((
                CheckboxBundle::new(theme, settings.developer.colliders, "Display colliders"),
                setting_field!(settings.developer.colliders),
            ));
            parent.spawn((
                CheckboxBundle::new(theme, settings.developer.wireframe, "Display wireframe"),
                setting_field!(settings.developer.wireframe),
            ));
            parent.spawn((
                CheckboxBundle::new(theme, settings.developer.paths, "Display navigation paths"),
                setting_field!(settings.developer.paths),
            ));
            parent.spawn((
                CheckboxBundle::new(
                    theme,
                    settings.developer.nav_mesh,
                    "Display navigation mesh",
                ),
                setting_field!(settings.developer.nav_mesh),
            ));
        });
}

// Creates a settings menu node.
#[derive(Default, Event)]
pub(super) struct SettingsMenuOpen;

#[derive(Component)]
struct SettingsMenu;

#[derive(Default, Display, EnumIter, PartialEq)]
enum SettingsTab {
    #[default]
    Video,
    Controls,
    Developer,
}

#[derive(Clone, Component, Copy, Display, EnumIter, PartialEq)]
enum SettingsButton {
    Ok,
    Cancel,
}

#[derive(Clone, Component, Copy, Display, EnumIter, PartialEq)]
enum BindingDialogButton {
    Replace,
    Delete,
    Cancel,
}

/// Stores information about button mapping.
#[derive(Component)]
struct Mapping {
    action: Action,
    input_kind: Option<InputKind>,
}

/// Contains button entity that was selected for binding.
#[derive(Component)]
struct BindingButton(Entity);

/// Contains button entity that has the same `input_kind` as the [`BindingButton`].
#[derive(Component)]
struct ConflictButton(Entity);

/// Marker for label with binding dialog text.
#[derive(Component)]
struct BindingLabel;

#[derive(Component)]
struct SettingsField(&'static str);
