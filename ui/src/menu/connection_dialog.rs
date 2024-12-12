use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_renet::renet::RenetClient;

use project_harmonia_widgets::{
    button::TextButtonBundle, click::Click, dialog::DialogBundle, label::LabelBundle, theme::Theme,
};

pub(super) struct ConnectionDialogPlugin;

impl Plugin for ConnectionDialogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Self::read_clicks).add_systems(
            Update,
            (
                Self::show.run_if(client_started_connecting),
                Self::close.run_if(client_just_disconnected.or_else(client_just_connected)),
            ),
        );
    }
}

impl ConnectionDialogPlugin {
    fn show(
        mut commands: Commands,
        theme: Res<Theme>,
        roots: Query<Entity, (With<Node>, Without<Parent>)>,
    ) {
        info!("showing connection dialog");
        commands.entity(roots.single()).with_children(|parent| {
            parent
                .spawn((ConnectionDialog, DialogBundle::new(&theme)))
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
                            parent.spawn(LabelBundle::normal(&theme, "Connecting to server"));
                            parent
                                .spawn((CancelButton, TextButtonBundle::normal(&theme, "Cancel")));
                        });
                });
        });
    }

    fn read_clicks(
        mut commands: Commands,
        mut click_events: EventReader<Click>,
        buttons: Query<(), With<CancelButton>>,
    ) {
        for _ in buttons.iter_many(click_events.read().map(|event| event.0)) {
            info!("cancelling connection");
            commands.remove_resource::<RenetClient>();
        }
    }

    fn close(mut commands: Commands, dialogs: Query<Entity, With<ConnectionDialog>>) {
        if let Ok(entity) = dialogs.get_single() {
            // Dialog may not be created if the connection happens instantly.
            info!("closing connection dialog");
            commands.entity(entity).despawn_recursive();
        }
    }
}

#[derive(Component)]
struct CancelButton;

#[derive(Component)]
struct ConnectionDialog;
