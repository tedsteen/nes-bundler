use std::time::{Duration, Instant};

use egui::{Align, Button, Color32, FontId, Label, RichText, TextEdit, Ui, Widget};
use serde::Deserialize;

use crate::{
    bundle::Bundle,
    emulation::LocalNesState,
    gui::MenuButton,
    main_view::gui::MainGui,
    netplay::{connecting_state::StartMethod, netplay_state::MAX_ROOM_NAME_LEN},
};

use super::{
    connecting_state::Connecting,
    netplay_state::{Connected, Netplay, NetplayState},
    ConnectingState, NetplayStateHandler,
};
#[cfg(feature = "debug")]
mod debug;

#[derive(Deserialize, Debug)]
pub struct NetplayVoca {
    pub name: String,
}
impl Default for NetplayVoca {
    fn default() -> Self {
        Self {
            name: "Netplay".to_string(),
        }
    }
}

pub struct NetplayGui {
    #[cfg(feature = "debug")]
    pub stats: [debug::NetplayStats; crate::settings::MAX_PLAYERS],
    room_name: Option<String>,
    last_screen: Option<&'static str>,
}

impl NetplayGui {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "debug")]
            stats: [debug::NetplayStats::new(), debug::NetplayStats::new()],
            room_name: None,
            last_screen: None,
        }
    }
}

fn ui_text_small(text: impl Into<String>, color: Color32) -> RichText {
    RichText::new(text)
        .color(color)
        .strong()
        .font(FontId::monospace(15.0))
}

impl NetplayGui {
    pub fn messages(&self, netplay_state_handler: &NetplayStateHandler) -> Option<Vec<String>> {
        Some(
            match &netplay_state_handler.netplay {
                Some(NetplayState::Connecting(Netplay { state })) => match state {
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
                    _ => None,
                },
                Some(NetplayState::Resuming(_)) => {
                    Some("Connection lost, trying to reconnect".to_string())
                }
                _ => None,
            }
            .iter()
            .map(|s| format!("{} - {s}", self.name().expect("a name")))
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
            }

            let mut action = None;

            ui.vertical_centered(|ui| {
                Label::new(MenuButton::ui_text(
                    "JOIN PRIVATE GAME",
                    MenuButton::ACTIVE_COLOR,
                ))
                .selectable(false)
                .ui(ui);
            });
            ui.end_row();

            ui.vertical_centered(|ui| {
                Label::new(ui_text_small("ENTER CODE", MenuButton::ACTIVE_COLOR))
                    .selectable(false)
                    .ui(ui);
            });
            ui.end_row();
            ui.add_space(10.0);

            ui.end_row();

            let enter_pressed_in_room_input = ui
                .vertical_centered(|ui| {
                    let re = ui.add(
                        TextEdit::singleline(room_name)
                            .horizontal_align(Align::Center)
                            .font(FontId::monospace(30.0))
                            .desired_width(30.0 * 3.0)
                            .vertical_align(Align::Center),
                    );
                    ui.add_space(10.0);

                    if ui
                        .add_enabled(
                            !room_name.is_empty(),
                            Button::new(RichText::new("Join").font(FontId::proportional(30.0))),
                        )
                        .clicked()
                    {
                        action = Some(Action::Join(room_name.clone()));
                    }
                    if !self.last_screen.eq(&Some("JOIN")) {
                        re.request_focus();
                    }
                    if re.lost_focus() && re.ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
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

            if room_name.len() > 4 {
                *room_name = room_name[..MAX_ROOM_NAME_LEN.into()].to_string();
            }
            *room_name = room_name.to_uppercase();

            if enter_pressed_in_room_input {
                action = Some(Action::Join(room_name.clone()));
            }

            ui.end_row();

            self.last_screen = Some("JOIN");

            if let Some(action) = action {
                self.room_name = None;
                match action {
                    Action::Join(room_name) => {
                        return netplay_disconnected
                            .join_game(&room_name)
                            .expect("to be able to join game");
                    }
                }
            }
        } else {
            enum Action {
                Find,
                Join,
                Host,
            }

            let mut action = None;

            ui.vertical_centered(|ui| {
                if MenuButton::new("FIND PUBLIC GAME").ui(ui).clicked() {
                    action = Some(Action::Find);
                }
            });
            ui.end_row();

            ui.vertical_centered(|ui| {
                if MenuButton::new("HOST PRIVATE GAME").ui(ui).clicked() {
                    action = Some(Action::Host);
                }
            });
            ui.end_row();

            ui.vertical_centered(|ui| {
                if MenuButton::new("JOIN PRIVATE GAME").ui(ui).clicked() {
                    action = Some(Action::Join);
                }
            });
            ui.end_row();

            self.last_screen = Some("DISCONNECTED");

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

        match &netplay_connecting.state {
            ConnectingState::LoadingNetplayServerConfiguration(Connecting {
                start_method, ..
            })
            | ConnectingState::PeeringUp(Connecting { start_method, .. }) => match start_method {
                super::connecting_state::StartMethod::Start(.., room_name, join_or_host) => {
                    match join_or_host {
                        super::connecting_state::JoinOrHost::Join => {
                            ui.vertical_centered(|ui| {
                                Label::new(MenuButton::ui_text(
                                    "JOINING PRIVATE GAME",
                                    MenuButton::ACTIVE_COLOR,
                                ))
                                .selectable(false)
                                .ui(ui);
                            });
                        }

                        super::connecting_state::JoinOrHost::Host => {
                            ui.vertical_centered(|ui| {
                                Label::new(MenuButton::ui_text(
                                    "HOSTING PRIVATE GAME",
                                    MenuButton::ACTIVE_COLOR,
                                ))
                                .selectable(false)
                                .ui(ui);
                            });
                        }
                    }
                    ui.end_row();

                    ui.vertical_centered(|ui| {
                        Label::new(ui_text_small(
                            "WAITING FOR SECOND PLAYER",
                            MenuButton::ACTIVE_COLOR,
                        ))
                        .selectable(false)
                        .ui(ui);
                    });

                    ui.end_row();
                    ui.add_space(10.0);
                    ui.end_row();

                    ui.vertical_centered(|ui| {
                        Label::new(MenuButton::ui_text("CODE", MenuButton::ACTIVE_COLOR))
                            .selectable(false)
                            .ui(ui);
                    });
                    ui.end_row();
                    ui.vertical_centered(|ui| {
                        Label::new(MenuButton::ui_text(
                            room_name,
                            Color32::from_rgb(255, 225, 0),
                        ))
                        .ui(ui);
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
                            "FINDING PUBLIC GAME",
                            MenuButton::ACTIVE_COLOR,
                        ))
                        .selectable(false)
                        .ui(ui);
                    });
                    ui.end_row();

                    ui.vertical_centered(|ui| {
                        Label::new(ui_text_small(
                            "WAITING FOR SECOND PLAYER",
                            MenuButton::ACTIVE_COLOR,
                        ))
                        .selectable(false)
                        .ui(ui);
                    });
                }
            },
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
                                action =
                                    Some(Action::Retry(synchronizing_state.start_method.clone()));
                            }
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
        ui.vertical(|ui| {
            ui.add_space(20.0);
        });
        ui.end_row();

        ui.vertical_centered(|ui| {
            if ui.button("Disconnect").clicked() {
                action = Some(Action::Cancel);
            }
        });

        if let Some(action) = action {
            match action {
                Action::Cancel => {
                    MainGui::set_main_menu_visibility(false);
                    return NetplayState::Disconnected(netplay_connecting.cancel());
                }
                Action::Retry(start_method) => {
                    return netplay_connecting.cancel().start(start_method);
                }
            }
        }
        NetplayState::Connecting(netplay_connecting)
    }

    fn ui_connected(&mut self, ui: &mut Ui, netplay_connected: Netplay<Connected>) -> NetplayState {
        enum Action {
            FakeDisconnect,
            Disconnect,
        }
        let mut action = None;
        if Instant::now()
            .duration_since(netplay_connected.state.start_time)
            .as_millis()
            < 200
        {
            MainGui::set_main_menu_visibility(false);
        }
        #[cfg(not(feature = "debug"))]
        let fake_lost_connection_clicked = false;
        #[cfg(feature = "debug")]
        {
            ui.vertical_centered(|ui| {
                ui.collapsing("Stats", |ui| {
                    Self::stats_ui(ui, &self.stats[0], 0);
                    Self::stats_ui(ui, &self.stats[1], 1);
                });
                if ui.button("Fake connection lost").clicked() {
                    action = Some(Action::FakeDisconnect);
                }
            });
            ui.end_row();
        }

        ui.vertical_centered(|ui| {
            Label::new(MenuButton::ui_text("CONNECTED!", MenuButton::ACTIVE_COLOR))
                .selectable(false)
                .ui(ui);
        });
        ui.end_row();

        ui.vertical_centered(|ui| {
            if ui.button("Disconnect").clicked() {
                action = Some(Action::Disconnect);
            }
        });
        ui.end_row();

        if let Some(action) = action {
            match action {
                Action::FakeDisconnect => {
                    log::debug!("Manually resuming connection (faking a lost connection)");
                    return NetplayState::Resuming(netplay_connected.resume());
                }
                Action::Disconnect => {
                    return NetplayState::Disconnected(netplay_connected.disconnect());
                }
            }
        }
        NetplayState::Connected(netplay_connected)
    }

    pub fn ui(&mut self, ui: &mut Ui, netplay_state_handler: &mut NetplayStateHandler) {
        let netplay = &mut netplay_state_handler.netplay;
        *netplay = Some(match netplay.take().unwrap() {
            NetplayState::Disconnected(netplay_disconnected) => {
                let res = self.ui_disconnected(ui, netplay_disconnected);
                ui.end_row();
                ui.vertical_centered(|ui| {
                    if Button::new(RichText::new("Close").font(FontId::proportional(20.0)))
                        .ui(ui)
                        .clicked()
                    {
                        self.room_name = None;
                        MainGui::set_main_menu_visibility(false);
                    }
                });
                res
            }
            NetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(ui, netplay_connecting)
            }
            NetplayState::Connected(netplay_connected) => self.ui_connected(ui, netplay_connected),
            NetplayState::Resuming(netplay_resuming) => {
                let mut disconnect_clicked = false;

                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("RESUMING...", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
                ui.vertical_centered(|ui| {
                    disconnect_clicked = ui.button("Disconnect").clicked();
                });

                ui.end_row();

                if disconnect_clicked {
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

    pub fn name(&self) -> Option<&str> {
        Some(&Bundle::current().config.vocabulary.netplay.name)
    }
}
