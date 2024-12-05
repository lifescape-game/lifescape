use bevy::{prelude::*, ui};

pub(crate) struct ClickPlugin;

impl Plugin for ClickPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<Click>()
            .add_systems(PreUpdate, Self::emit_clicks.after(ui::ui_focus_system));
    }
}

impl ClickPlugin {
    fn emit_clicks(
        mut click_events: EventWriter<Click>,
        mut buttons: Query<(Entity, &Interaction, &mut LastInteraction), Changed<Interaction>>,
    ) {
        for (entity, &interaction, mut last_interaction) in &mut buttons {
            if last_interaction.0 == Interaction::Pressed {
                click_events.send(Click(entity));
            }
            last_interaction.0 = interaction;
        }
    }
}

/// Happens when RMB was clicked and released on a button.
///
/// Currently [`Interaction::Click`] state is basically a pressed state of the button and not an actual "click".
#[derive(Event)]
pub struct Click(pub Entity);

/// Holds previous [`Interaction`] that is needed for [`ButtonClick`] event.
#[derive(Component, Default)]
pub(super) struct LastInteraction(Interaction);
