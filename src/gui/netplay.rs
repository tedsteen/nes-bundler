use egui::{Button, Context, Window, TextEdit};

use crate::{
    settings::{Settings, MAX_PLAYERS}, network::{NetplayState, connect},
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
        Self {
            room_name: "example_room".to_string()
        }
    }
}
