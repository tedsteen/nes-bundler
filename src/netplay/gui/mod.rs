use std::time::{Duration, Instant};

use egui::{Align, Button, Color32, FontId, Label, RichText, TextEdit, Ui, Widget};
use serde::Deserialize;

use crate::{
    bundle::Bundle,
    gui::{MenuButton, esc_pressed},
    main_view::gui::MainMenuState,
    netplay::{
        MAX_ROOM_NAME_LEN, NetplayCommand, NetplayCommandBus, SharedNetplay,
        SharedNetplayConnectedState, SharedNetplayState,
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

enum DisconnectedState {
    Main,
    JoiningWithCode { room_name: String, focus_requested: bool },
}

pub struct NetplayGui {
    disconnected_state: DisconnectedState,
    shared_netplay: SharedNetplay,
}

impl NetplayGui {
    pub fn new(shared_netplay: SharedNetplay) -> Self {
        Self {
            disconnected_state: DisconnectedState::Main,
            shared_netplay,
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
    pub fn ui(&mut self, ui: &mut Ui) -> Option<MainMenuState> {
        let netplay_tx = &self.shared_netplay.command_tx.clone();

        match &*self.shared_netplay.receiver.clone().borrow() {
            SharedNetplayState::Disconnected => {
                return self.ui_disconnected(ui);
            }
            SharedNetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(ui, &netplay_connecting.borrow());
            }
            SharedNetplayState::Connected(netplay_connected) => {
                let nav = self.ui_connected(ui, &netplay_connected, &netplay_tx.clone());
                #[cfg(feature = "debug")]
                {
                    ui.vertical_centered(|ui| {
                        ui.collapsing("Stats", |ui| {
                            let stats = &*self.shared_netplay.stats.read().unwrap();
                            Self::stats_ui(ui, &stats[0], 0);
                            Self::stats_ui(ui, &stats[1], 1);
                        });
                        if ui.button("Fake connection lost").clicked() {
                            let _ = netplay_tx.try_send(NetplayCommand::RetryConnect);
                        }
                    });
                    ui.end_row();
                }
                return nav;
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
                    return Some(MainMenuState::Main);
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
                    return Some(MainMenuState::Main);
                }
            }
        };
        None
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

    pub fn messages(&self, menu_state: &MainMenuState) -> Option<Vec<String>> {
        if matches!(menu_state, MainMenuState::Netplay) {
            // No need to show messages when the netplay menu is already showing status
            return None;
        }

        match &*self.shared_netplay.receiver.borrow() {
            SharedNetplayState::Resuming => Some(vec![format!(
                "{} - Trying to reconnect...",
                self.name().expect("a name")
            )]),
            _ => None,
        }
    }

    fn ui_disconnected(&mut self, ui: &mut Ui) -> Option<MainMenuState> {
        enum Transition {
            Stay,
            EnterJoinCode,
            ReturnToMain,
            JoinGame(String),
        }

        // Clone the sender upfront so closures inside the match arms don't
        // need to borrow through `self` while `self.disconnected_state` is matched on.
        let command_tx = self.shared_netplay.command_tx.clone();

        let (transition, nav) = match &mut self.disconnected_state {
            DisconnectedState::JoiningWithCode { room_name, focus_requested } => {
                let mut join_action: Option<String> = None;

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
                                Button::new(
                                    RichText::new("Join").font(FontId::proportional(30.0)),
                                ),
                            )
                            .clicked()
                        {
                            join_action = Some(room_name.clone());
                        }
                        if !*focus_requested {
                            re.request_focus();
                            *focus_requested = true;
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

                if room_name.len() > MAX_ROOM_NAME_LEN.into() {
                    *room_name = room_name[..MAX_ROOM_NAME_LEN.into()].to_string();
                }
                *room_name = room_name.to_uppercase();

                if enter_pressed_in_room_input {
                    join_action = Some(room_name.clone());
                }

                ui.end_row();
                let mut transition = Transition::Stay;
                ui.vertical_centered(|ui| {
                    if ui_button("Cancel").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                        transition = Transition::ReturnToMain;
                    }
                });

                if let Some(name) = join_action {
                    transition = Transition::JoinGame(name);
                }
                (transition, None)
            }
            DisconnectedState::Main => {
                let netplay_voca = &Bundle::current().config.vocabulary.netplay;
                let mut transition = Transition::Stay;
                let mut nav = None;

                if !netplay_voca.find_public_game.is_empty() {
                    ui.vertical_centered(|ui| {
                        if MenuButton::new(netplay_voca.find_public_game.clone())
                            .ui(ui)
                            .clicked()
                        {
                            let _ = command_tx.try_send(NetplayCommand::FindGame);
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
                            let _ = command_tx.try_send(NetplayCommand::HostGame);
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
                            transition = Transition::EnterJoinCode;
                        }
                    });
                    ui.end_row();
                }

                ui.vertical_centered(|ui| {
                    if ui_button("Close").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                        nav = Some(MainMenuState::Main);
                    }
                });

                (transition, nav)
            }
        };

        match transition {
            Transition::Stay => {}
            Transition::EnterJoinCode => {
                self.disconnected_state = DisconnectedState::JoiningWithCode {
                    room_name: String::new(),
                    focus_requested: false,
                };
            }
            Transition::ReturnToMain => {
                self.disconnected_state = DisconnectedState::Main;
            }
            Transition::JoinGame(name) => {
                self.disconnected_state = DisconnectedState::Main;
                let _ = self
                    .shared_netplay
                    .command_tx
                    .try_send(NetplayCommand::JoinGame(name));
            }
        }

        nav
    }

    fn ui_connecting(&mut self, ui: &mut Ui, netplay_connecting: &ConnectingState) {
        let netplay_voca = &Bundle::current().config.vocabulary.netplay;
        match netplay_connecting {
                ConnectingState::LoadingNetplayServerConfiguration => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("CONNECTING", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
            }
            ConnectingState::PeeringUp(start_method) => {
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

        ui.end_row();

        ui.vertical(|ui| {
            ui.add_space(20.0);
        });
        ui.end_row();

        ui.vertical_centered(|ui| {
            if ui_button("Cancel").ui(ui).clicked() || esc_pressed(ui.ctx()) {
                let _ = self
                    .shared_netplay
                    .command_tx
                    .try_send(NetplayCommand::CancelConnect);
            }
        });
    }

    fn ui_connected(
        &mut self,
        ui: &mut Ui,
        netplay_connected: &SharedNetplayConnectedState,
        netplay_tx: &NetplayCommandBus,
    ) -> Option<MainMenuState> {
        let mut nav = None;
        match &netplay_connected {
            SharedNetplayConnectedState::Running(start_time) => {
                // Hide menu if we just managed to connect
                if Instant::now().duration_since(*start_time).as_millis() < 200 {
                    nav = Some(MainMenuState::Closed);
                }

                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("CONNECTED!", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
            }
            SharedNetplayConnectedState::Synchronizing(start_time, unlock_url) => {
                ui.vertical_centered(|ui| {
                    Label::new(MenuButton::ui_text("PAIRING UP", MenuButton::ACTIVE_COLOR))
                        .selectable(false)
                        .ui(ui);
                });
                ui.end_row();
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
                            let _ = self
                                .shared_netplay
                                .command_tx
                                .try_send(NetplayCommand::RetryConnect);
                        }
                    });
                }
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
            nav = Some(MainMenuState::Main);
        }
        nav
    }

    pub fn name(&self) -> Option<&str> {
        Some(&Bundle::current().config.vocabulary.netplay.name)
    }
}
