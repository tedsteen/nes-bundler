use egui::{Button, Context, Window, TextEdit};

use crate::{
    settings::{Settings, MAX_PLAYERS}, network::{NetplayState},
};

use super::GuiComponent;
pub(crate) struct NetplayGui {
}

impl GuiComponent for NetplayGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _settings: &mut Settings) {}
    fn ui(&mut self, ctx: &Context, settings: &mut Settings) {
        Window::new("Netplay!").collapsible(false).show(ctx, |ui| {
            match &mut settings.netplay.state {
                NetplayState::Disconnected => {
                    egui::Grid::new("my_grid")
                    .num_columns(2)
                    .spacing([10.0, 4.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Join a game by name");
                        ui.add(TextEdit::singleline(&mut settings.netplay.room_name).desired_width(140.0)
                            .hint_text("Name of network game"));
                        if ui
                            .add_enabled(!settings.netplay.room_name.is_empty(), Button::new("Join"))
                            .on_disabled_hover_text("Which network game do you want to join?")
                            .clicked()
                        {
                            settings.netplay.connect(&settings.netplay.room_name.clone());
                        }
                        ui.end_row();
                        ui.label("or simply");
                        if ui.button("Match with a random player").clicked() {
                            settings.netplay.connect("beta-0?next=2");
                        }
                        ui.end_row();
                        ui.label("Max prediction (frames)");
                        ui.add(egui::DragValue::new(&mut settings.netplay.max_prediction).speed(1.0).clamp_range(1..=20));
                        ui.end_row();
                        ui.label("Input delay (frames)");
                        ui.add(egui::DragValue::new(&mut settings.netplay.input_delay).speed(1.0).clamp_range(1..=7));
                        ui.end_row();

                    });
                }
                NetplayState::Connecting(s) => {
                    if let Some(socket) = s {
                        let connected_peers = socket.connected_peers().len();
                        let remaining = MAX_PLAYERS - (connected_peers + 1);
                        ui.label(format!("Waiting for {} players", remaining));
                        //TODO: Cancel button
                    }
                }
                NetplayState::Connected(_session) => {
                    //TODO: Disconnect button
                },
            }
        });
    }
}

impl NetplayGui {
    pub(crate) fn new() -> Self {
        Self { }
    }
}
