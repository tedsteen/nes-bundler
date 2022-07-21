use egui::{Button, Context, TextEdit, Window};

use crate::{network::NetplayState, settings::MAX_PLAYERS, GameRunner};

use super::GuiComponent;
pub struct NetplayGui {
    is_open: bool,
}

impl GuiComponent for NetplayGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _: &mut GameRunner) {}
    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        Window::new("Netplay!")
            .open(&mut self.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let netplay = &mut game_runner.netplay;
                match &mut netplay.state {
                    NetplayState::Disconnected => {
                        egui::Grid::new("netplay_grid")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Join a game by name");
                                ui.add(
                                    TextEdit::singleline(&mut netplay.room_name)
                                        .desired_width(140.0)
                                        .hint_text("Name of network game"),
                                );
                                if ui
                                    .add_enabled(!netplay.room_name.is_empty(), Button::new("Join"))
                                    .on_disabled_hover_text(
                                        "Which network game do you want to join?",
                                    )
                                    .clicked()
                                {
                                    netplay.connect(&netplay.room_name.clone());
                                }
                                ui.end_row();
                                ui.label("or simply");
                                if ui.button("Match with a random player").clicked() {
                                    netplay.connect("beta-0?next=2");
                                }
                                ui.end_row();
                                ui.label("Max prediction (frames)");
                                ui.add(
                                    egui::DragValue::new(&mut netplay.max_prediction)
                                        .speed(1.0)
                                        .clamp_range(1..=20),
                                );
                                ui.end_row();
                                ui.label("Input delay (frames)");
                                ui.add(
                                    egui::DragValue::new(&mut netplay.input_delay)
                                        .speed(1.0)
                                        .clamp_range(1..=7),
                                );
                                ui.end_row();
                            });
                    }
                    NetplayState::Connecting(s) => {
                        if let Some(socket) = s {
                            let connected_peers = socket.connected_peers().len();
                            let remaining = MAX_PLAYERS - (connected_peers + 1);
                            ui.label(format!("Waiting for {} players", remaining));
                            if ui.button("Cancel").clicked() {
                                netplay.state = NetplayState::Disconnected;
                            }
                        }
                    }
                    NetplayState::Connected(_) => {
                        if ui.button("Disconnect").clicked() {
                            netplay.state = NetplayState::Disconnected;
                        }
                    }
                }
            });
    }

    fn is_open(&mut self) -> &mut bool {
        &mut self.is_open
    }

    fn name(&self) -> String {
        "Netplay!".to_string()
    }
}

impl NetplayGui {
    pub fn new() -> Self {
        Self { is_open: false }
    }
}
