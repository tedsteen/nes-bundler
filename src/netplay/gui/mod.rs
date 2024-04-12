use std::time::{Duration, Instant};

use egui::{Align, Color32, Label, TextEdit, Ui, Widget};

use crate::{
    emulation::LocalNesState,
    gui::MenuButton,
    netplay::connecting_state::StartMethod,
    settings::{gui::SettingsGui, MAX_PLAYERS},
};

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
                    ConnectingState::Synchronizing(_) => Some("Pairing up...".to_string()),
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
                        Label::new(MenuButton::ui_text("JOIN GAME", MenuButton::ACTIVE_COLOR))
                            .selectable(false)
                            .ui(ui);
                    });
                    ui.end_row();

                    ui.vertical_centered(|ui| {
                        Label::new(MenuButton::ui_text_small(
                            "ENTER CODE",
                            MenuButton::ACTIVE_COLOR,
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
                self.room_name = None;
                match action {
                    Action::Join(room_name) => {
                        return netplay_disconnected
                            .join_game(&room_name)
                            .expect("to be able to join game");
                    }
                    Action::Cancel => {}
                }
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
                        Label::new(MenuButton::ui_text("PUBLIC", MenuButton::INACTIVE_COLOR))
                            .selectable(false)
                            .ui(ui);
                        ui.end_row();

                        ui.vertical_centered(|ui| {
                            if MenuButton::new("FIND GAME").ui(ui).clicked() {
                                action = Some(Action::Find);
                            }
                        });
                        ui.end_row();

                        Label::new(MenuButton::ui_text("PRIVATE", MenuButton::INACTIVE_COLOR))
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
        enum Action {
            Cancel,
            Retry(StartMethod),
        }
        let mut action = None;

        egui::Grid::new("netplay_connecting_menu_grid")
            .num_columns(1)
            .spacing([10.0, 10.0])
            .show(ui, |ui| {
                match &netplay_connecting.state {
                    ConnectingState::LoadingNetplayServerConfiguration(Connecting {
                        start_method,
                        ..
                    })
                    | ConnectingState::PeeringUp(Connecting { start_method, .. }) => {
                        match start_method {
                            super::connecting_state::StartMethod::Start(
                                ..,
                                room_name,
                                join_or_host,
                            ) => {
                                match join_or_host {
                                    super::connecting_state::JoinOrHost::Join => {
                                        ui.vertical_centered(|ui| {
                                            Label::new(MenuButton::ui_text(
                                                "JOINING GAME",
                                                MenuButton::ACTIVE_COLOR,
                                            ))
                                            .selectable(false)
                                            .ui(ui);
                                        });
                                    }

                                    super::connecting_state::JoinOrHost::Host => {
                                        ui.vertical_centered(|ui| {
                                            Label::new(MenuButton::ui_text(
                                                "HOSTING GAME",
                                                MenuButton::ACTIVE_COLOR,
                                            ))
                                            .selectable(false)
                                            .ui(ui);
                                        });
                                    }
                                }
                                ui.end_row();

                                ui.vertical_centered(|ui| {
                                    Label::new(MenuButton::ui_text_small(
                                        "WAITING FOR SECOND PLAYER",
                                        MenuButton::ACTIVE_COLOR,
                                    ))
                                    .selectable(false)
                                    .ui(ui);
                                });

                                ui.end_row();

                                ui.vertical_centered(|ui| {
                                    Label::new(MenuButton::ui_text(
                                        "CODE",
                                        MenuButton::ACTIVE_COLOR,
                                    ))
                                    .selectable(false)
                                    .ui(ui);
                                });
                                ui.end_row();
                                ui.vertical_centered(|ui| {
                                    Label::new(MenuButton::ui_text(
                                        room_name,
                                        Color32::from_rgb(255, 200, 200),
                                    ))
                                    .ui(ui);
                                });
                                ui.end_row();
                                ui.vertical(|ui| {
                                    ui.add_space(20.0);
                                });
                            }
                            super::connecting_state::StartMethod::Resume(_) => {
                                ui.vertical_centered(|ui| {
                                    Label::new(MenuButton::ui_text(
                                        "RESUMING GAME",
                                        MenuButton::ACTIVE_COLOR,
                                    ))
                                    .selectable(false)
                                    .ui(ui);
                                });
                            }
                            super::connecting_state::StartMethod::MatchWithRandom(_) => {
                                ui.vertical_centered(|ui| {
                                    Label::new(MenuButton::ui_text(
                                        "FINDING GAME",
                                        MenuButton::ACTIVE_COLOR,
                                    ))
                                    .selectable(false)
                                    .ui(ui);
                                });
                                ui.end_row();

                                ui.vertical_centered(|ui| {
                                    Label::new(MenuButton::ui_text_small(
                                        "WAITING FOR SECOND PLAYER",
                                        MenuButton::ACTIVE_COLOR,
                                    ))
                                    .selectable(false)
                                    .ui(ui);
                                });
                                ui.end_row();
                                ui.vertical(|ui| {
                                    ui.add_space(20.0);
                                });
                            }
                        }
                    }
                    ConnectingState::Synchronizing(synchronizing_state) => {
                        ui.vertical_centered(|ui| {
                            Label::new(MenuButton::ui_text(
                                "PAIRING UP...",
                                MenuButton::ACTIVE_COLOR,
                            ))
                            .selectable(false)
                            .ui(ui);
                        });
                        ui.end_row();
                        if let Some(unlock_url) = &synchronizing_state.state.unlock_url {
                            if Instant::now()
                                .duration_since(synchronizing_state.state.start_time)
                                .gt(&Duration::from_secs(5))
                            {
                                ui.vertical_centered(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        ui.label("We're having trouble connecting you, click ");
                                        ui.hyperlink_to("here", unlock_url);
                                        ui.label(" to unlock Netplay!");
                                    });
                                });
                                ui.end_row();
                                ui.vertical_centered(|ui| {
                                    if ui.button("Retry").clicked() {
                                        action = Some(Action::Retry(
                                            synchronizing_state.start_method.clone(),
                                        ));
                                    }
                                });
                                ui.end_row();
                                ui.vertical(|ui| {
                                    ui.add_space(20.0);
                                });
                            }
                        }
                    }
                    // NOTE: This captures failed, retrying and connected. Let's just show "CONNECTING..." during that state
                    _ => {
                        ui.vertical_centered(|ui| {
                            Label::new(MenuButton::ui_text(
                                "CONNECTING...",
                                MenuButton::ACTIVE_COLOR,
                            ))
                            .selectable(false)
                            .ui(ui);
                        });
                    }
                }
                ui.end_row();

                ui.vertical_centered(|ui| {
                    if ui.button("Cancel").clicked() {
                        action = Some(Action::Cancel);
                    }
                });
            });
        if let Some(action) = action {
            match action {
                Action::Cancel => return NetplayState::Disconnected(netplay_connecting.cancel()),
                Action::Retry(start_method) => {
                    return netplay_connecting.cancel().start(start_method);
                }
            }
        }
        NetplayState::Connecting(netplay_connecting)
    }

    fn ui_connected(&mut self, ui: &mut Ui, netplay_connected: Netplay<Connected>) -> NetplayState {
        if Instant::now()
            .duration_since(netplay_connected.state.start_time)
            .as_millis()
            < 200
        {
            SettingsGui::set_main_menu_visibility(false);
        }
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

        let mut disconnect_clicked = false;
        egui::Grid::new("netplay_connected_menu_grid")
            .num_columns(1)
            .spacing([10.0, 10.0])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("CONNECTED!", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
                ui.vertical_centered(|ui| {
                    disconnect_clicked = ui.button("Disconnect").clicked();
                });

                ui.end_row();
            });
        if disconnect_clicked {
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
                let mut cancel_clicked = false;
                egui::Grid::new("netplay_resuming_menu_grid")
                    .num_columns(1)
                    .spacing([10.0, 10.0])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            Label::new(MenuButton::ui_text(
                                "RESUMING...",
                                MenuButton::ACTIVE_COLOR,
                            ))
                            .selectable(false)
                            .ui(ui);
                        });
                        ui.end_row();
                        ui.vertical_centered(|ui| {
                            cancel_clicked = ui.button("Cancel").clicked();
                        });

                        ui.end_row();
                    });

                if cancel_clicked {
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
