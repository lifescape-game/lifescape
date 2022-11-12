use bevy::prelude::*;
use bevy_egui::egui::{epaint::WHITE_UV, Align, Image, Layout, TextureId, Ui};
use iyes_loopless::prelude::*;

use crate::core::{
    family::{FamilyDelete, FamilySelect, Members},
    game_state::GameState,
    network::network_event::client_event::ClientSendBuffer,
};

pub(super) struct FamiliesTab<'a, 'w, 's, 'we, 'se, 'wq, 'sq> {
    commands: &'a mut Commands<'w, 's>,
    select_buffer: &'a mut EventWriter<'we, 'se, FamilySelect>,
    delete_buffer: &'a mut ClientSendBuffer<FamilyDelete>,
    families: &'a Query<'wq, 'sq, (Entity, &'static Name), With<Members>>,
}

impl<'a, 'w, 's, 'we, 'se, 'wq, 'sq> FamiliesTab<'a, 'w, 's, 'we, 'se, 'wq, 'sq> {
    #[must_use]
    pub(super) fn new(
        commands: &'a mut Commands<'w, 's>,
        delete_buffer: &'a mut ClientSendBuffer<FamilyDelete>,
        select_buffer: &'a mut EventWriter<'we, 'se, FamilySelect>,
        families: &'a Query<'wq, 'sq, (Entity, &'static Name), With<Members>>,
    ) -> Self {
        Self {
            families,
            select_buffer,
            delete_buffer,
            commands,
        }
    }
}

impl FamiliesTab<'_, '_, '_, '_, '_, '_, '_> {
    pub(super) fn show(self, ui: &mut Ui) {
        for (entity, name) in self.families {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        Image::new(TextureId::Managed(0), (64.0, 64.0)).uv([WHITE_UV, WHITE_UV]),
                    );
                    ui.label(name.as_str());
                    ui.with_layout(Layout::top_down(Align::Max), |ui| {
                        if ui.button("⏵ Play").clicked() {
                            self.select_buffer.send(FamilySelect(entity));
                        }
                        if ui.button("🗑 Delete").clicked() {
                            self.delete_buffer.push(FamilyDelete(entity));
                        }
                    })
                });
            });
        }
        ui.with_layout(Layout::left_to_right(Align::Max), |ui| {
            if ui.button("➕ Create new").clicked() {
                self.commands
                    .insert_resource(NextState(GameState::FamilyEditor));
            }
            ui.allocate_space(ui.available_size_before_wrap());
        });
    }
}
