use std::time::{Duration, Instant};

use egui::{Align, Button, Color32, FontId, Label, RichText, TextEdit, Ui, Widget};
use serde::Deserialize;

use crate::{
    bundle::Bundle,
    emulation::Emulator,
    gui::{MenuButton, esc_pressed},
    main_view::gui::{MainGui, MainMenuState},
    netplay::{
        MAX_ROOM_NAME_LEN, NetplayCommand, NetplayCommandBus, SharedNetplayConnectedState,
        SharedNetplayState,
        connection::{ConnectingState, StartMethod},
    },
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
    pub fn ui(&mut self, ui: &mut Ui, emulator: &mut Emulator) {
        let netplay_tx = &emulator.shared_state.netplay.command_tx;
        let shared_netplay_state = emulator.shared_state.netplay.receiver.borrow();

        match &*shared_netplay_state {
            SharedNetplayState::Disconnected => {
                self.ui_disconnected(ui, &emulator.shared_state.netplay.command_tx);
            }
            SharedNetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(
                    ui,
                    &netplay_connecting.borrow(),
                    &emulator.shared_state.netplay.command_tx,
                );
            }
            SharedNetplayState::Connected(netplay_connected) => {
                self.ui_connected(
                    ui,
                    netplay_connected,
                    &emulator.shared_state.netplay.command_tx,
                );
                #[cfg(feature = "debug")]
                {
                    ui.vertical_centered(|ui| {
                        ui.collapsing("Stats", |ui| {
                            let stats = &*emulator.shared_state.netplay.stats.read().unwrap();
                            Self::stats_ui(ui, &stats[0], 0);
                            Self::stats_ui(ui, &stats[1], 1);
                        });
                        if ui.button("Fake connection lost").clicked() {
                            use crate::netplay::NetplayCommand;

                            let _ = netplay_tx.try_send(NetplayCommand::Resume);
                        }
                    });
                    ui.end_row();
                }
            }
            SharedNetplayState::Resuming => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("RESUMING...", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
                if ui
                    .vertical_centered(|ui| ui_button("Disconnect").ui(ui).clicked())
                    .inner
                {
                    let _ = netplay_tx.try_send(NetplayCommand::CancelConnect);
                }
                ui.end_row();

                if esc_pressed(ui.ctx()) {
                    MainGui::set_main_menu_state(MainMenuState::Main);
                }
            }
            SharedNetplayState::Failed(reason) => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text(
                        "CONNECTION FAILED",
                        MenuButton::ACTIVE_COLOR,
                    ))
                    .selectable(false)
                    .ui(ui);
                });
                ui.end_row();
                ui.vertical_centered(|ui| {
                    Label::new(ui_text_small(reason, MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
                if ui
                    .vertical_centered(|ui| ui_button("Retry").ui(ui).clicked())
                    .inner
                {
                    let _ = netplay_tx.try_send(NetplayCommand::RetryConnect);
                }
                ui.end_row();
                ui.vertical_centered(|ui| {
                    if ui_button("Cancel").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                        let _ = netplay_tx.try_send(NetplayCommand::CancelConnect);
                    }
                });
                ui.end_row();
                if esc_pressed(ui.ctx()) {
                    MainGui::set_main_menu_state(MainMenuState::Main);
                }
            }
        };
    }

    fn needs_unlocking(start_time: Instant, unlock_url: &Option<String>) -> Option<String> {
        if let Some(unlock_url) = unlock_url {
            if Instant::now()
                .duration_since(start_time)
                .gt(&Duration::from_secs(5))
            {
                return Some(unlock_url.clone());
            }
        }
        None
    }
    pub fn messages(&self, emulator: &Emulator) -> Option<Vec<String>> {
        if matches!(MainGui::main_menu_state(), MainMenuState::Netplay) {
            // No need to show messages when the netplay menu is already showing status
            return None;
        }

        Some(
            match &*emulator.shared_state.netplay.receiver.borrow() {
                // Connecting is a modal state, you can't see any messages when in the netplay UI anyway
                SharedNetplayState::Connecting(_) => None,
                SharedNetplayState::Resuming => Some("Trying to reconnect...".to_string()),
                _ => None,
            }
            .iter()
            .map(|msg| format!("{} - {msg}", self.name().expect("a name")))
            .collect(),
        )
    }

    fn ui_disconnected(&mut self, ui: &mut Ui, netplay_tx: &NetplayCommandBus) {
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
                        let _ = netplay_tx.try_send(NetplayCommand::JoinGame(room_name));
                    }
                }
            }
        } else {
            let netplay_voca = &Bundle::current().config.vocabulary.netplay;

            if !netplay_voca.find_public_game.is_empty() {
                ui.vertical_centered(|ui| {
                    if MenuButton::new(netplay_voca.find_public_game.clone())
                        .ui(ui)
                        .clicked()
                    {
                        let _ = netplay_tx.try_send(NetplayCommand::FindGame);
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
                        let _ = netplay_tx.try_send(NetplayCommand::HostGame);
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
                        self.room_name = Some(String::new())
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
        }
    }

    fn ui_connecting(
        &mut self,
        ui: &mut Ui,
        netplay_connecting: &ConnectingState,
        netplay_tx: &NetplayCommandBus,
    ) {
        let netplay_voca = &Bundle::current().config.vocabulary.netplay;
        match netplay_connecting {
            ConnectingState::Idle => {
                //Will never be this state. TODO: Remove it
            }
            ConnectingState::LoadingNetplayServerConfiguration => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("CONNECTING", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
            }
            ConnectingState::PeeringUp(start_method, unlock_url, start_time) => {
                if let Some(unlock_url) = Self::needs_unlocking(*start_time, unlock_url) {
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
                            let _ = netplay_tx.send(NetplayCommand::RetryConnect);
                        }
                    });
                } else {
                    match start_method {
                        StartMethod::Start(.., room_name, join_or_host) => {
                            use super::connection::JoinOrHost::*;
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
                        StartMethod::MatchWithRandom => {
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
                    }
                }
            }
        }

        ui.end_row();

        ui.vertical(|ui| {
            ui.add_space(20.0);
        });
        ui.end_row();

        ui.vertical_centered(|ui| {
            if ui_button("Cancel").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                let _ = netplay_tx.try_send(NetplayCommand::CancelConnect);
            }
        });
    }

    fn ui_connected(
        &mut self,
        ui: &mut Ui,
        netplay_connected: &SharedNetplayConnectedState,
        netplay_tx: &NetplayCommandBus,
    ) {
        match &netplay_connected {
            SharedNetplayConnectedState::Running(start_time) => {
                // Hide menu if we just managed to connect
                if Instant::now().duration_since(*start_time).as_millis() < 200 {
                    MainGui::set_main_menu_state(MainMenuState::Closed);
                }

                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("CONNECTED!", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
            }
            SharedNetplayConnectedState::Synchronizing => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("PAIRING UP", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
            }
        }
        ui.vertical_centered(|ui| {
            if ui_button("Disconnect").ui(ui).clicked() {
                let _ = netplay_tx.try_send(NetplayCommand::Disconnect);
            }
        });
        ui.end_row();

        if esc_pressed(ui.ctx()) {
            MainGui::set_main_menu_state(MainMenuState::Main);
        }
    }

    pub fn name(&self) -> Option<&str> {
        Some(&Bundle::current().config.vocabulary.netplay.name)
    }
}
