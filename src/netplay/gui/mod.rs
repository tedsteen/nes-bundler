use std::time::{Duration, Instant};

use egui::{Align, Button, Color32, FontId, Label, RichText, TextEdit, Ui, Widget};
use serde::Deserialize;

use crate::{
    bundle::Bundle,
    emulation::{
        Emulator, NetplayCommandBus, SharedNetplayConnectedState, SharedNetplayConnectingState,
        SharedNetplayState,
    },
    gui::{MenuButton, esc_pressed},
    main_view::gui::{MainGui, MainMenuState},
    netplay::{
        connecting_state::{StartMethod, SynchonizingState},
        netplay_state::MAX_ROOM_NAME_LEN,
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
    pub fn messages(&self, emulator: &Emulator) -> Option<Vec<String>> {
        if matches!(MainGui::main_menu_state(), MainMenuState::Netplay) {
            // No need to show messages when the netplay menu is already showing status
            return None;
        }

        Some(
            match &*emulator.shared_state.netplay_state.read().unwrap() {
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
                        let _ = netplay_tx
                            .try_send(crate::emulation::NetplayCommand::JoinGame(room_name));
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
                        let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::FindGame);
                    }
                    Action::Join => self.room_name = Some(String::new()),
                    Action::Host => {
                        let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::HostGame);
                    }
                }
            }
        }
    }

    fn ui_connecting(
        &mut self,
        ui: &mut Ui,
        netplay_connecting: &SharedNetplayConnectingState,
        netplay_tx: &NetplayCommandBus,
    ) {
        enum Action {
            Cancel,
            Retry(StartMethod),
        }
        let mut action = None;
        let netplay_voca = &Bundle::current().config.vocabulary.netplay;
        match &netplay_connecting {
            SharedNetplayConnectingState::LoadingNetplayServerConfiguration(start_method, ..)
            | SharedNetplayConnectingState::PeeringUp(start_method, ..) => match start_method {
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
            SharedNetplayConnectingState::Synchronizing(synchronizing_state) => {
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
            SharedNetplayConnectingState::Failed(reason) => {
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
            SharedNetplayConnectingState::Connecting | SharedNetplayConnectingState::Retry => {
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
                    let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::CancelConnect);
                }
                Action::Retry(start_method) => {
                    let _ = netplay_tx
                        .send(crate::emulation::NetplayCommand::RetryConnect(start_method));
                }
            }
        }
    }

    fn ui_connected(
        &mut self,
        ui: &mut Ui,
        netplay_connected: &SharedNetplayConnectedState,
        netplay_tx: &NetplayCommandBus,
    ) {
        // Hide menu if we just managed to connect
        if Instant::now()
            .duration_since(netplay_connected.start_time)
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
                //TODO: Re-enable this
                // ui.collapsing("Stats", |ui| {
                //     Self::stats_ui(ui, &netplay_connected.state.stats[0], 0);
                //     Self::stats_ui(ui, &netplay_connected.state.stats[1], 1);
                // });
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
                    let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::Resume);
                }
                Action::Disconnect => {
                    let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::Disconnect);
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, emulator: &mut Emulator) {
        let netplay_tx = &emulator.shared_state.netplay_command_tx;
        match &*emulator.shared_state.netplay_state.read().unwrap() {
            SharedNetplayState::Disconnected => {
                self.ui_disconnected(ui, &emulator.shared_state.netplay_command_tx);
            }
            SharedNetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(
                    ui,
                    netplay_connecting,
                    &emulator.shared_state.netplay_command_tx,
                );
            }
            SharedNetplayState::Connected(netplay_connected) => {
                self.ui_connected(
                    ui,
                    netplay_connected,
                    &emulator.shared_state.netplay_command_tx,
                );
            }
            SharedNetplayState::Resuming => {
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
                    let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::CancelConnect);
                }
            }
            SharedNetplayState::Failed(reason) => {
                ui.label(format!("Failed to connect: {}", reason));
                if ui.button("Ok").clicked() || esc_pressed(ui.ctx()) {
                    let _ = netplay_tx.try_send(crate::emulation::NetplayCommand::Disconnect);
                }
            }
        };
    }

    pub fn name(&self) -> Option<&str> {
        Some(&Bundle::current().config.vocabulary.netplay.name)
    }
}
