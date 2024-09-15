use bevy::prelude::*;
use project_harmonia_base::game_world::{
    actor::{
        needs::{Need, NeedGlyph},
        SelectedActor,
    },
    WorldState,
};
use project_harmonia_widgets::{
    button::{ExclusiveButton, TabContent, TextButtonBundle, Toggled},
    label::LabelBundle,
    progress_bar::{ProgressBar, ProgressBarBundle},
    theme::Theme,
};
use strum::{EnumIter, IntoEnumIterator};

pub(super) struct InfoNodePlugin;

impl Plugin for InfoNodePlugin {
    fn build(&self, app: &mut App) {
        app.observe(Self::cleanup_need_bars).add_systems(
            Update,
            Self::update_need_bars.run_if(in_state(WorldState::Family)),
        );
    }
}

impl InfoNodePlugin {
    fn update_need_bars(
        mut commands: Commands,
        theme: Res<Theme>,
        needs: Query<(Entity, &NeedGlyph, Ref<Need>)>,
        actors: Query<(&Children, Ref<SelectedActor>)>,
        tabs: Query<(&TabContent, &InfoTab)>,
        mut progress_bars: Query<(&mut ProgressBar, &BarNeed)>,
    ) {
        let (children, selected_actor) = actors.single();
        let (tab_content, _) = tabs
            .iter()
            .find(|(_, &tab)| tab == InfoTab::Needs)
            .expect("tab with cities should be spawned on state enter");

        if selected_actor.is_added() {
            commands.entity(tab_content.0).despawn_descendants();
        }

        for (entity, glyph, need) in needs
            .iter_many(children)
            .filter(|(.., need)| need.is_changed() || selected_actor.is_added())
        {
            if let Some((mut progress_bar, _)) = progress_bars
                .iter_mut()
                .find(|(_, bar_need)| bar_need.0 == entity)
            {
                trace!("updating bar with `{need:?}` for `{entity}`");
                progress_bar.0 = need.0;
            } else {
                trace!("creating bar with `{need:?}` for `{entity}`");
                commands.entity(tab_content.0).with_children(|parent| {
                    parent.spawn(LabelBundle::symbol(&theme, glyph.0));
                    parent.spawn((BarNeed(entity), ProgressBarBundle::new(&theme, need.0)));
                });
            }
        }
    }

    fn cleanup_need_bars(
        trigger: Trigger<OnRemove, Need>,
        mut commands: Commands,
        progress_bars: Query<(Entity, &BarNeed)>,
    ) {
        if let Some((entity, _)) = progress_bars
            .iter()
            .find(|(_, bar_need)| bar_need.0 == trigger.entity())
        {
            debug!("despawning bar `{entity}` for need `{}`", trigger.entity());
            commands.entity(entity).despawn_recursive();
        }
    }
}

pub(super) fn setup(parent: &mut ChildBuilder, tab_commands: &mut Commands, theme: &Theme) {
    parent
        .spawn(NodeBundle {
            style: Style {
                flex_direction: FlexDirection::ColumnReverse,
                position_type: PositionType::Absolute,
                align_self: AlignSelf::FlexEnd,
                right: Val::Px(0.0),
                ..Default::default()
            },
            ..Default::default()
        })
        .with_children(|parent| {
            let tabs_entity = parent
                .spawn(NodeBundle {
                    style: Style {
                        padding: theme.padding.normal,
                        align_self: AlignSelf::FlexEnd,
                        ..Default::default()
                    },
                    background_color: theme.panel_color.into(),
                    ..Default::default()
                })
                .id();

            for (index, tab) in InfoTab::iter().enumerate() {
                let tab_content = match tab {
                    InfoTab::Needs => parent
                        .spawn(NodeBundle {
                            style: Style {
                                display: Display::Grid,
                                width: Val::Px(400.0),
                                column_gap: theme.gap.normal,
                                row_gap: theme.gap.normal,
                                padding: theme.padding.normal,
                                grid_template_columns: vec![
                                    GridTrack::auto(),
                                    GridTrack::flex(1.0),
                                    GridTrack::auto(),
                                    GridTrack::flex(1.0),
                                ],
                                ..Default::default()
                            },
                            background_color: theme.panel_color.into(),

                            ..Default::default()
                        })
                        .id(),
                    InfoTab::Skills => parent.spawn(NodeBundle::default()).id(),
                };

                tab_commands
                    .spawn((
                        tab,
                        TabContent(tab_content),
                        ExclusiveButton,
                        Toggled(index == 0),
                        TextButtonBundle::symbol(theme, tab.glyph()),
                    ))
                    .set_parent(tabs_entity);
            }
        });
}

#[derive(Component)]
struct BarNeed(Entity);

#[derive(Component, EnumIter, Clone, Copy, PartialEq)]
enum InfoTab {
    Needs,
    Skills,
}

impl InfoTab {
    fn glyph(self) -> &'static str {
        match self {
            InfoTab::Needs => "📈",
            InfoTab::Skills => "💡",
        }
    }
}
