use std::time::{Duration, Instant};

use egui::{Align, Button, Color32, FontId, Label, RichText, TextEdit, Ui, Widget};
use serde::Deserialize;

use crate::{
    bundle::Bundle,
    emulation::LocalNesState,
    gui::{MenuButton, esc_pressed},
    main_view::gui::{MainGui, MainMenuState},
    netplay::{
        connecting_state::{
            ConnectingState, SharedConnectingState, StartMethod, SynchonizingState,
        },
        netplay_state::MAX_ROOM_NAME_LEN,
    },
};

use super::{
    NetplayStateHandler,
    netplay_state::{ConnectedState, Netplay, NetplayState},
};
#[cfg(feature = "debug")]
mod debug;

#[derive(Deserialize, Debug)]
pub struct NetplayVoca {
    pub name: String,
    pub find_public_game: String,
    pub host_private_game: String,
    pub join_private_game: String,
    pub finding_public_game: String,
    pub hosting_private_game: String,
    pub joining_private_game: String,
}

impl Default for NetplayVoca {
    fn default() -> Self {
        Self {
            name: "Netplay".to_string(),
            find_public_game: "FIND PUBLIC GAME".to_string(),
            host_private_game: "HOST PRIVATE GAME".to_string(),
            join_private_game: "JOIN PRIVATE GAME".to_string(),
            finding_public_game: "FINDING PUBLIC GAME".to_string(),
            hosting_private_game: "HOSTING PRIVATE GAME".to_string(),
            joining_private_game: "JOINING PRIVATE GAME".to_string(),
        }
    }
}

pub struct NetplayGui {
    room_name: Option<String>,
    last_screen: Option<&'static str>,
}

impl NetplayGui {
    pub fn new() -> Self {
        Self {
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

fn ui_button(text: &str) -> Button<'_> {
    Button::new(RichText::new(text).font(FontId::proportional(20.0)))
}

impl NetplayGui {
    fn needs_unlocking(synchronizing_state: &SynchonizingState) -> Option<&str> {
        if let Some(unlock_url) = &synchronizing_state.netplay_server_configuration.unlock_url {
            if Instant::now()
                .duration_since(synchronizing_state.start_time)
                .gt(&Duration::from_secs(5))
            {
                return Some(unlock_url);
            }
        }
        None
    }
    pub fn messages(&self, netplay_state_handler: &NetplayStateHandler) -> Option<Vec<String>> {
        if matches!(MainGui::main_menu_state(), MainMenuState::Netplay) {
            // No need to show messages when the netplay menu is already showing status
            return None;
        }

        Some(
            match &netplay_state_handler.netplay {
                // Connecting is a modal state, you can't see any messages when in the netplay UI anyway
                Some(NetplayState::Connecting(_)) => None,
                Some(NetplayState::Resuming(_)) => Some("Trying to reconnect...".to_string()),
                _ => None,
            }
            .iter()
            .map(|msg| format!("{} - {msg}", self.name().expect("a name")))
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
                    Bundle::current()
                        .config
                        .vocabulary
                        .netplay
                        .join_private_game
                        .clone(),
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
            ui.vertical_centered(|ui| {
                if ui_button("Cancel").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                    self.room_name = None;
                }
            });
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
            let netplay_voca = &Bundle::current().config.vocabulary.netplay;

            if !netplay_voca.find_public_game.is_empty() {
                ui.vertical_centered(|ui| {
                    if MenuButton::new(netplay_voca.find_public_game.clone())
                        .ui(ui)
                        .clicked()
                    {
                        action = Some(Action::Find);
                    }
                });
                ui.end_row();
            }

            if !netplay_voca.host_private_game.is_empty() {
                ui.vertical_centered(|ui| {
                    if MenuButton::new(netplay_voca.host_private_game.clone())
                        .ui(ui)
                        .clicked()
                    {
                        action = Some(Action::Host);
                    }
                });
                ui.end_row();
            }

            if !netplay_voca.join_private_game.is_empty() {
                ui.vertical_centered(|ui| {
                    if MenuButton::new(netplay_voca.join_private_game.clone())
                        .ui(ui)
                        .clicked()
                    {
                        action = Some(Action::Join);
                    }
                });
                ui.end_row();
            }
            ui.vertical_centered(|ui| {
                if ui_button("Close").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                    self.room_name = None;
                    MainGui::set_main_menu_state(MainMenuState::Main);
                }
            });

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
        netplay_connecting: Netplay<SharedConnectingState>,
    ) -> NetplayState {
        enum Action {
            Cancel,
            Retry(StartMethod),
        }
        let mut action = None;
        let netplay_voca = &Bundle::current().config.vocabulary.netplay;
        match &*netplay_connecting.state.borrow() {
            ConnectingState::LoadingNetplayServerConfiguration(start_method, ..)
            | ConnectingState::PeeringUp(start_method, ..) => match start_method {
                StartMethod::Start(.., room_name, join_or_host) => {
                    use super::connecting_state::JoinOrHost::*;
                    match join_or_host {
                        Join => {
                            ui.vertical_centered(|ui| {
                                Label::new(MenuButton::ui_text(
                                    netplay_voca.joining_private_game.clone(),
                                    MenuButton::ACTIVE_COLOR,
                                ))
                                .selectable(false)
                                .ui(ui);
                            });
                        }

                        Host => {
                            ui.vertical_centered(|ui| {
                                Label::new(MenuButton::ui_text(
                                    netplay_voca.hosting_private_game.clone(),
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
                StartMethod::MatchWithRandom(_) => {
                    ui.vertical_centered(|ui| {
                        Label::new(MenuButton::ui_text(
                            netplay_voca.finding_public_game.clone(),
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
                StartMethod::Resume(..) => {
                    //This is used internally during the `NetplayState::Resuming` state
                }
            },
            ConnectingState::Synchronizing(synchronizing_state) => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("PAIRING UP", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
                if let Some(unlock_url) = Self::needs_unlocking(&synchronizing_state) {
                    ui.vertical_centered(|ui| {
                        ui.set_width(300.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.label("We're having trouble connecting you, click ");
                            ui.hyperlink_to("here", unlock_url)
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            ui.label(" to unlock Netplay!");
                        });
                    });
                    ui.end_row();

                    ui.vertical_centered(|ui| {
                        if ui.button("Retry").clicked() {
                            action = Some(Action::Retry(synchronizing_state.start_method.clone()));
                        }
                    });
                }
            }
            ConnectingState::Failed(reason) => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text(
                        "FAILED TO CONNECT",
                        MenuButton::ACTIVE_COLOR,
                    ))
                    .selectable(false)
                    .ui(ui);
                });
                ui.end_row();

                ui.vertical_centered(|ui| {
                    Label::new(ui_text_small(reason, MenuButton::ACTIVE_COLOR)).ui(ui);
                });
            }
            // NOTE: This captures retrying and connected. Let's just show "CONNECTING" during that state
            _ => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("CONNECTING", MenuButton::ACTIVE_COLOR))
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
            if ui_button("Disconnect").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                action = Some(Action::Cancel);
            }
        });

        if let Some(action) = action {
            match action {
                Action::Cancel => {
                    return NetplayState::Disconnected(netplay_connecting.cancel());
                }
                Action::Retry(start_method) => {
                    return netplay_connecting.cancel().start(start_method);
                }
            }
        }
        NetplayState::Connecting(netplay_connecting)
    }

    fn ui_connected(
        &mut self,
        ui: &mut Ui,
        netplay_connected: Netplay<ConnectedState>,
    ) -> NetplayState {
        // Hide menu if we just managed to connect
        if Instant::now()
            .duration_since(netplay_connected.state.start_time)
            .as_millis()
            < 200
        {
            MainGui::set_main_menu_state(MainMenuState::Closed);
        }

        ui.vertical_centered(|ui| {
            Label::new(MenuButton::ui_text("CONNECTED!", MenuButton::ACTIVE_COLOR))
                .selectable(false)
                .ui(ui);
        });
        ui.end_row();

        #[allow(dead_code)] // Some actions are only triggered by certain features
        enum Action {
            FakeDisconnect,
            Disconnect,
        }

        let mut action = None;
        ui.vertical_centered(|ui| {
            if ui_button("Disconnect").ui(ui).clicked() {
                action = Some(Action::Disconnect);
            }
        });
        ui.end_row();

        if esc_pressed(ui.ctx()) {
            MainGui::set_main_menu_state(MainMenuState::Main);
        }

        #[cfg(feature = "debug")]
        {
            ui.vertical_centered(|ui| {
                ui.collapsing("Stats", |ui| {
                    Self::stats_ui(ui, &netplay_connected.state.stats[0], 0);
                    Self::stats_ui(ui, &netplay_connected.state.stats[1], 1);
                });
                if ui.button("Fake connection lost").clicked() {
                    action = Some(Action::FakeDisconnect);
                }
            });
            ui.end_row();
        }

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
                self.ui_disconnected(ui, netplay_disconnected)
            }
            NetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(ui, netplay_connecting)
            }
            NetplayState::Connected(netplay_connected) => self.ui_connected(ui, netplay_connected),
            NetplayState::Resuming(netplay_resuming) => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("RESUMING...", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
                let disconnect_clicked = ui
                    .vertical_centered(|ui| ui_button("Disconnect").ui(ui).clicked())
                    .inner;
                ui.end_row();

                if esc_pressed(ui.ctx()) {
                    MainGui::set_main_menu_state(MainMenuState::Main);
                }

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
                if ui.button("Ok").clicked() || esc_pressed(ui.ctx()) {
                    NetplayState::Disconnected(netplay_failed.disconnect())
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
