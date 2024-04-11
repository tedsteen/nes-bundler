use std::time::{Duration, Instant};

use egui::{Align, Label, TextEdit, Ui, Widget};

use crate::{emulation::LocalNesState, gui::MenuButton, settings::MAX_PLAYERS};

use super::{
    connecting_state::{Connecting, PeeringState},
    netplay_state::{Connected, Netplay, NetplayState},
    ConnectingState, NetplayStateHandler,
};
#[cfg(feature = "debug")]
mod debug;

pub struct NetplayGui {
    #[cfg(feature = "debug")]
    pub stats: [debug::NetplayStats; MAX_PLAYERS],
    room_name: Option<String>,
}

impl NetplayGui {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "debug")]
            stats: [debug::NetplayStats::new(), debug::NetplayStats::new()],
            room_name: None,
        }
    }
}

impl NetplayGui {
    pub fn messages(&self, netplay_state_handler: &NetplayStateHandler) -> Option<Vec<String>> {
        Some(
            match &netplay_state_handler.netplay {
                Some(NetplayState::Connecting(Netplay { state })) => match state {
                    ConnectingState::LoadingNetplayServerConfiguration(_) => {
                        Some("Initialising".to_string())
                    }
                    ConnectingState::PeeringUp(Connecting::<PeeringState> {
                        state: PeeringState { socket, .. },
                        ..
                    }) => {
                        let connected_peers = socket.connected_peers().count();
                        let remaining = MAX_PLAYERS - (connected_peers + 1);
                        Some(format!("Waiting for {remaining} player...."))
                    }
                    ConnectingState::Synchronizing(_) => Some("Synchronising".to_string()),
                    ConnectingState::Connected(_) => None,
                    ConnectingState::Retrying(retrying) => Some(format!(
                        "Connection failed ({}), retrying in {}s...",
                        retrying.state.fail_message,
                        retrying
                            .state
                            .deadline
                            .duration_since(Instant::now())
                            .as_secs()
                            + 1
                    )),
                    ConnectingState::Failed(reason) => Some(format!("Failed ({reason})")),
                },
                Some(NetplayState::Resuming(_)) => {
                    Some("Connection lost, trying to reconnect".to_string())
                }
                _ => None,
            }
            .iter()
            .map(|s| format!("Netplay - {s}"))
            .collect(),
        )
    }

    fn ui_disconnected(
        &mut self,
        ui: &mut Ui,
        netplay_disconnected: Netplay<LocalNesState>,
    ) -> NetplayState {
        if let Some(room_name) = &mut self.room_name {
            enum Action {
                Join(String),
                Cancel,
            }

            let mut action = None;

            egui::Grid::new("netplay_join_menu_grid")
                .num_columns(1)
                .spacing([10.0, 10.0])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        Label::new(MenuButton::ui_text("JOIN GAME", MenuButton::UNACTIVE_COLOR))
                            .selectable(false)
                            .ui(ui);
                    });
                    ui.end_row();

                    ui.vertical_centered(|ui| {
                        Label::new(MenuButton::ui_text_small(
                            "ENTER CODE",
                            MenuButton::UNACTIVE_COLOR,
                        ))
                        .selectable(false)
                        .ui(ui);
                    });
                    ui.end_row();

                    let enter_pressed_in_room_input = ui
                        .vertical_centered(|ui| {
                            let re = ui.add(
                                TextEdit::singleline(room_name)
                                    .horizontal_align(Align::Center)
                                    .desired_width(140.0)
                                    .vertical_align(Align::Center),
                            );
                            if re.lost_focus() && re.ctx.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                if room_name.is_empty() {
                                    re.request_focus();
                                    false
                                } else {
                                    true
                                }
                            } else {
                                false
                            }
                        })
                        .inner;
                    ui.end_row();

                    let room_name = room_name.clone();

                    if enter_pressed_in_room_input {
                        action = Some(Action::Join(room_name));
                    }
                    ui.vertical_centered(|ui| {
                        if ui.button("Cancel").clicked() {
                            action = Some(Action::Cancel);
                        }
                    });
                    ui.end_row();
                });
            if let Some(action) = action {
                match action {
                    Action::Join(room_name) => {
                        return netplay_disconnected
                            .join_game(&room_name)
                            .expect("to be able to join game");
                    }
                    Action::Cancel => {}
                }
                self.room_name = None;
            }
        } else {
            enum Action {
                Find,
                Join,
                Host,
            }

            let mut action = None;

            ui.add_space(20.0);
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                egui::Grid::new("netplay_menu_grid")
                    .num_columns(1)
                    .spacing([10.0, 10.0])
                    .show(ui, |ui| {
                        Label::new(MenuButton::ui_text("PUBLIC", MenuButton::UNACTIVE_COLOR))
                            .selectable(false)
                            .ui(ui);
                        ui.end_row();

                        ui.vertical_centered(|ui| {
                            if MenuButton::new("FIND GAME").ui(ui).clicked() {
                                action = Some(Action::Find);
                            }
                        });
                        ui.end_row();

                        Label::new(MenuButton::ui_text("PRIVATE", MenuButton::UNACTIVE_COLOR))
                            .selectable(false)
                            .ui(ui);
                        ui.end_row();

                        ui.vertical_centered(|ui| {
                            if MenuButton::new("HOST GAME").ui(ui).clicked() {
                                action = Some(Action::Host);
                            }
                        });
                        ui.end_row();

                        ui.vertical_centered(|ui| {
                            if MenuButton::new("JOIN GAME").ui(ui).clicked() {
                                action = Some(Action::Join);
                            }
                        });
                        ui.end_row();
                    });
            });
            ui.add_space(20.0);

            if let Some(action) = action {
                match action {
                    Action::Find => {
                        return netplay_disconnected
                            .find_game()
                            .expect("to be able to find a game");
                    }
                    Action::Join => self.room_name = Some(String::new()),
                    Action::Host => {
                        return netplay_disconnected
                            .host_game()
                            .expect("to be able to host a game");
                    }
                }
            }
        }

        NetplayState::Disconnected(netplay_disconnected)
    }

    fn ui_connecting(
        &mut self,
        ui: &mut Ui,
        netplay_connecting: Netplay<ConnectingState>,
    ) -> NetplayState {
        let mut retry_start_method = None;

        #[allow(clippy::collapsible_match)]
        match &netplay_connecting.state {
            ConnectingState::LoadingNetplayServerConfiguration(_) => {
                ui.label("Initializing...");
            }

            ConnectingState::PeeringUp(..) => {
                ui.label("Peering up...");
            }
            ConnectingState::Synchronizing(synchronizing_state) => {
                let start_method = synchronizing_state.start_method.clone();
                ui.label("Synchronizing players...");
                if let Some(unlock_url) = &synchronizing_state.state.unlock_url {
                    if Instant::now()
                        .duration_since(synchronizing_state.state.start_time)
                        .gt(&Duration::from_secs(5))
                    {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.label("We're having trouble connecting you, click ");
                            ui.hyperlink_to("here", unlock_url);
                            ui.label(" to unlock Netplay!");
                        });
                        if ui.button("Retry").clicked() {
                            retry_start_method = Some(start_method);
                        }
                    }
                }
            }
            ConnectingState::Retrying(_) => {
                ui.label("Retrying");
            }
            _ => {}
        }
        if let Some(start_method) = retry_start_method {
            netplay_connecting.cancel().start(start_method)
        } else if ui.button("Cancel").clicked() {
            NetplayState::Disconnected(netplay_connecting.cancel())
        } else {
            NetplayState::Connecting(netplay_connecting)
        }
    }

    fn ui_connected(&mut self, ui: &mut Ui, netplay_connected: Netplay<Connected>) -> NetplayState {
        #[cfg(not(feature = "debug"))]
        let fake_lost_connection_clicked = false;
        #[cfg(feature = "debug")]
        let fake_lost_connection_clicked = {
            ui.collapsing("Stats", |ui| {
                Self::stats_ui(ui, &self.stats[0], 0);
                Self::stats_ui(ui, &self.stats[1], 1);
            });
            ui.button("Fake connection lost").clicked()
        };

        if ui.button("Disconnect").clicked() {
            NetplayState::Disconnected(netplay_connected.disconnect())
        } else if fake_lost_connection_clicked {
            log::debug!("Manually resuming connection (faking a lost connection)");
            NetplayState::Resuming(netplay_connected.resume())
        } else {
            NetplayState::Connected(netplay_connected)
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, netplay_state_handler: &mut NetplayStateHandler) {
        let netplay = &mut netplay_state_handler.netplay;
        *netplay = Some(match netplay.take().unwrap() {
            NetplayState::Disconnected(netplay_disconnected) => {
                self.ui_disconnected(ui, netplay_disconnected)
            }
            NetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(ui, netplay_connecting)
            }
            NetplayState::Connected(netplay_connected) => self.ui_connected(ui, netplay_connected),
            NetplayState::Resuming(netplay_resuming) => {
                ui.label("Trying to resume...");
                if ui.button("Cancel").clicked() {
                    NetplayState::Disconnected(netplay_resuming.cancel())
                } else {
                    NetplayState::Resuming(netplay_resuming)
                }
            }
            NetplayState::Failed(netplay_failed) => {
                ui.label(format!(
                    "Failed to connect: {}",
                    netplay_failed.state.reason
                ));
                if ui.button("Ok").clicked() {
                    NetplayState::Disconnected(netplay_failed.restart())
                } else {
                    NetplayState::Failed(netplay_failed)
                }
            }
        });
    }

    pub fn name(&self) -> Option<String> {
        Some("Netplay".to_string())
    }
}
