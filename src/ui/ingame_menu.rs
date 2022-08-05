use std::mem;

use bevy::{app::AppExit, prelude::*};
use bevy_egui::EguiContext;
use iyes_loopless::prelude::*;
use leafwing_input_manager::prelude::ActionState;

use crate::core::{
    game_state::GameState,
    game_world::{GameSaved, WorldName},
};

use super::{modal_window::ModalWindow, settings_menu::SettingsMenu, ui_action::UiAction};

pub(super) struct InGameMenuPlugin;

impl Plugin for InGameMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(
            Self::open_ingame_menu_system
                .run_in_state(GameState::InGame)
                .run_unless_resource_exists::<InGameMenu>(),
        )
        .add_exit_system(GameState::InGame, Self::close_ingame_menu)
        .add_system(Self::ingame_menu_system.run_if_resource_exists::<InGameMenu>())
        .add_system(Self::save_as_dialog_system.run_if_resource_exists::<SaveAsDialog>())
        .add_system(Self::exit_to_main_menu_system.run_if_resource_exists::<ExitToMainMenuDialog>())
        .add_system(Self::exit_game_system.run_if_resource_exists::<ExitGameDialog>());
    }
}

impl InGameMenuPlugin {
    fn open_ingame_menu_system(mut commands: Commands, action_state: Res<ActionState<UiAction>>) {
        if action_state.just_pressed(UiAction::Back) {
            commands.init_resource::<InGameMenu>();
        }
    }

    fn close_ingame_menu(mut commands: Commands) {
        commands.remove_resource::<InGameMenu>();
    }

    fn ingame_menu_system(
        mut commands: Commands,
        mut save_events: EventWriter<GameSaved>,
        mut egui: ResMut<EguiContext>,
        mut action_state: ResMut<ActionState<UiAction>>,
    ) {
        let mut open = true;
        ModalWindow::new(&mut open, &mut action_state, "Menu").show(egui.ctx_mut(), |ui| {
            ui.vertical_centered(|ui| {
                if ui.button("Save").clicked() {
                    save_events.send_default();
                    commands.remove_resource::<InGameMenu>();
                }
                if ui.button("Save as...").clicked() {
                    commands.init_resource::<SaveAsDialog>();
                }
                if ui.button("Settings").clicked() {
                    commands.init_resource::<SettingsMenu>();
                }
                if ui.button("Exit to main menu").clicked() {
                    commands.init_resource::<ExitToMainMenuDialog>();
                }
                if ui.button("Exit game").clicked() {
                    commands.init_resource::<ExitGameDialog>();
                }
            });
        });

        if !open {
            commands.remove_resource::<InGameMenu>();
        }
    }

    fn save_as_dialog_system(
        mut commands: Commands,
        mut save_events: EventWriter<GameSaved>,
        mut egui: ResMut<EguiContext>,
        mut action_state: ResMut<ActionState<UiAction>>,
        mut world_name: ResMut<WorldName>,
        mut save_as_menu: ResMut<SaveAsDialog>,
    ) {
        let mut open = true;
        ModalWindow::new(&mut open, &mut action_state, "Save as...").show(egui.ctx_mut(), |ui| {
            ui.text_edit_singleline(&mut save_as_menu.world_name);
            ui.horizontal(|ui| {
                if ui.button("Ok").clicked() {
                    world_name.0 = mem::take(&mut save_as_menu.world_name);
                    save_events.send_default();
                    commands.remove_resource::<SaveAsDialog>();
                }
                if ui.button("Cancel").clicked() {
                    commands.remove_resource::<SaveAsDialog>();
                }
            });
        });

        if !open {
            commands.remove_resource::<SaveAsDialog>();
        }
    }

    fn exit_to_main_menu_system(
        mut commands: Commands,
        mut save_events: EventWriter<GameSaved>,
        mut egui: ResMut<EguiContext>,
        mut action_state: ResMut<ActionState<UiAction>>,
    ) {
        let mut open = true;
        ModalWindow::new(&mut open, &mut action_state, "Exit to main menu").show(
            egui.ctx_mut(),
            |ui| {
                ui.label("Would you like to save your world before exiting to the main menu?");
                ui.horizontal(|ui| {
                    if ui.button("Save and exit").clicked() {
                        save_events.send_default();
                        commands.remove_resource::<ExitToMainMenuDialog>();
                        commands.insert_resource(NextState(GameState::Menu));
                    }
                    if ui.button("Exit to main menu").clicked() {
                        commands.remove_resource::<ExitToMainMenuDialog>();
                        commands.insert_resource(NextState(GameState::Menu));
                    }
                    if ui.button("Cancel").clicked() {
                        commands.remove_resource::<ExitToMainMenuDialog>();
                    }
                });
            },
        );

        if !open {
            commands.remove_resource::<ExitToMainMenuDialog>();
        }
    }

    fn exit_game_system(
        mut commands: Commands,
        mut save_events: EventWriter<GameSaved>,
        mut egui: ResMut<EguiContext>,
        mut action_state: ResMut<ActionState<UiAction>>,
        mut exit_events: EventWriter<AppExit>,
    ) {
        let mut open = true;
        ModalWindow::new(&mut open, &mut action_state, "Exit game").show(egui.ctx_mut(), |ui| {
            ui.label("Are you sure you want to exit the game?");
            ui.horizontal(|ui| {
                if ui.button("Save and exit").clicked() {
                    save_events.send_default();
                    exit_events.send_default();
                }
                if ui.button("Exit without saving").clicked() {
                    exit_events.send_default();
                }
                if ui.button("Cancel").clicked() {
                    commands.remove_resource::<ExitGameDialog>();
                }
            });
        });

        if !open {
            commands.remove_resource::<ExitGameDialog>();
        }
    }
}

#[derive(Default)]
struct InGameMenu;

struct SaveAsDialog {
    world_name: String,
}

impl FromWorld for SaveAsDialog {
    fn from_world(world: &mut World) -> Self {
        SaveAsDialog {
            world_name: world.resource::<WorldName>().0.clone(),
        }
    }
}

#[derive(Default)]
struct ExitToMainMenuDialog;

#[derive(Default)]
struct ExitGameDialog;
