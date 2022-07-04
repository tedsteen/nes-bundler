use egui::{Button, Context, Window, TextEdit};
use ggrs::SessionBuilder;

use crate::{
    settings::{Settings, MAX_PLAYERS}, network::{NetplayState, GGRSConfig, connect},
};

use super::GuiComponent;
pub(crate) struct NetplayGui {
    room_name: String
}

impl GuiComponent for NetplayGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _settings: &mut Settings) {}
    fn ui(&mut self, ctx: &Context, settings: &mut Settings) {
        Window::new("Netplay!").collapsible(false).show(ctx, |ui| {
            match &mut settings.netplay_state {
                NetplayState::Disconnected => {
                    ui.add(TextEdit::singleline(&mut self.room_name)
                            .hint_text("Name of room to join"));
                    if ui
                        .add_enabled(!self.room_name.is_empty(), Button::new("Join"))
                        .on_disabled_hover_text("What room do you want to join?")
                        .clicked()
                    {
                        settings.netplay_state = NetplayState::Connecting(Some(connect(&self.room_name)));
                    }
                }
                NetplayState::Connecting(s) => {
                    if let Some(socket) = s {
                        socket.accept_new_connections();
                        let connected_peers = socket.connected_peers().len();
                        let remaining = MAX_PLAYERS - (connected_peers + 1);
                        ui.label(format!("Waiting for {} players", remaining));
                        //TODO: Cancel button
                        if remaining == 0 {
                            let players = socket.players();

                            let max_prediction = 12;

                            let mut sess_build = SessionBuilder::<GGRSConfig>::new()
                                .with_num_players(MAX_PLAYERS)
                                .with_max_prediction_window(max_prediction)
                                .with_input_delay(2)
                                .with_fps(settings.fps as usize)
                                .expect("invalid fps");

                            for (i, player) in players.into_iter().enumerate() {
                                sess_build = sess_build
                                    .add_player(player, i)
                                    .expect("failed to add player");
                            }

                            settings.netplay_state = NetplayState::Connected(sess_build
                                .start_p2p_session(s.take().unwrap())
                                .expect("failed to start session"));
                        }
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
        Self {
            room_name: "example_room".to_string()
        }
    }
}
