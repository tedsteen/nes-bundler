use std::time::{Duration, Instant};

/// How long the laggy-connection warning stays up after the game last froze or rolled back far.
const LAG_WARNING: Duration = Duration::from_secs(5);

use egui::{Align2, Color32, FontId, TextEdit, Ui, Widget};
use serde::Deserialize;

use crate::{
    bundle::Bundle,
    gui::{MenuButton, action_button, actions_ui, detail_ui, esc_pressed, sizes, title_ui},
    main_view::gui::MainMenuState,
    netplay::{
        ConnectionPath, MAX_ROOM_NAME_LEN, NetplayCommand, NetplayCommandBus, SharedNetplay,
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
    JoiningWithCode {
        room_name: String,
        focus_requested: bool,
    },
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

impl NetplayGui {
    pub fn ui(&mut self, ui: &mut Ui) -> Option<MainMenuState> {
        let netplay_tx = self.shared_netplay.command_tx.clone();
        let receiver = self.shared_netplay.receiver.clone();

        match &*receiver.borrow() {
            SharedNetplayState::Disconnected => {
                return self.ui_disconnected(ui);
            }
            SharedNetplayState::Connecting(netplay_connecting) => {
                self.ui_connecting(ui, &netplay_connecting.borrow());
            }
            SharedNetplayState::Connected(netplay_connected) => {
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
                return self.ui_connected(ui, netplay_connected, &netplay_tx);
            }
            SharedNetplayState::Resuming => {
                title_ui(ui, "RESUMING...", MenuButton::ACTIVE_COLOR);
                actions_ui(ui, |ui| {
                    if action_button("Disconnect").ui(ui).clicked() {
                        let _ = netplay_tx.try_send(NetplayCommand::CancelConnect);
                    }
                });
                if esc_pressed(ui) {
                    return Some(MainMenuState::Main);
                }
            }
            SharedNetplayState::Failed(reason, unlock_url) => {
                let esc_was_pressed = esc_pressed(ui);
                title_ui(ui, "CONNECTION FAILED", MenuButton::ACTIVE_COLOR);
                detail_ui(ui, reason);
                if let Some(unlock_url) = unlock_url {
                    Self::unlock_prompt_ui(ui, unlock_url);
                }
                actions_ui(ui, |ui| {
                    if action_button("Retry").ui(ui).clicked() {
                        let _ = netplay_tx.try_send(NetplayCommand::RetryConnect);
                    }
                    if action_button("Cancel").ui(ui).clicked() || esc_was_pressed {
                        let _ = netplay_tx.try_send(NetplayCommand::CancelConnect);
                    }
                });
                if esc_was_pressed {
                    return Some(MainMenuState::Main);
                }
            }
        };
        None
    }

    /// Explains that a relay could make the connection possible, and links to unlocking one.
    fn unlock_prompt_ui(ui: &mut Ui, unlock_url: &str) {
        ui.vertical_centered(|ui| {
            ui.set_width(sizes::TEXT_WIDTH);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.label("We're having trouble connecting you, click ");
                ui.hyperlink_to("here", unlock_url)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                ui.label(" to unlock Netplay!");
            });
        });
        ui.end_row();
    }

    fn retry_button_ui(&self, ui: &mut Ui) {
        if action_button("Retry").ui(ui).clicked() {
            let _ = self
                .shared_netplay
                .command_tx
                .try_send(NetplayCommand::RetryConnect);
        }
    }

    fn needs_unlocking(start_time: Instant, unlock_url: &Option<String>) -> Option<String> {
        unlock_url.as_ref().and_then(|unlock_url| {
            (Instant::now().duration_since(start_time) > Duration::from_secs(5))
                .then(|| unlock_url.clone())
        })
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
            SharedNetplayState::Connected(SharedNetplayConnectedState::Running(
                _,
                _,
                Some(last_lag),
            )) if last_lag.elapsed() < LAG_WARNING => Some(vec![format!(
                "{} - Laggy connection",
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
            DisconnectedState::JoiningWithCode {
                room_name,
                focus_requested,
            } => {
                let mut join_action: Option<String> = None;

                title_ui(
                    ui,
                    Bundle::current()
                        .config
                        .vocabulary
                        .netplay
                        .join_private_game
                        .clone(),
                    MenuButton::ACTIVE_COLOR,
                );
                detail_ui(ui, "ENTER CODE");

                let enter_pressed_in_room_input = ui
                    .vertical_centered(|ui| {
                        let re = ui.add(
                            TextEdit::singleline(room_name)
                                .align(Align2::CENTER_CENTER)
                                .font(FontId::monospace(sizes::TITLE))
                                .desired_width(sizes::TITLE * 3.0),
                        );
                        ui.add_space(sizes::ROW_SPACING);

                        if ui
                            .add_enabled(!room_name.is_empty(), action_button("Join"))
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

                let max_room_name_len = usize::from(MAX_ROOM_NAME_LEN);
                *room_name = room_name
                    .chars()
                    .take(max_room_name_len)
                    .collect::<String>()
                    .to_uppercase();

                if enter_pressed_in_room_input {
                    join_action = Some(room_name.clone());
                }

                let mut transition = Transition::Stay;
                actions_ui(ui, |ui| {
                    if action_button("Cancel").ui(ui).clicked() || esc_pressed(ui) {
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

                let menu_item = |ui: &mut Ui, text: &str| {
                    let clicked = !text.is_empty()
                        && ui
                            .vertical_centered(|ui| MenuButton::new(text).ui(ui).clicked())
                            .inner;
                    if !text.is_empty() {
                        ui.end_row();
                    }
                    clicked
                };
                if menu_item(ui, &netplay_voca.find_public_game) {
                    let _ = command_tx.try_send(NetplayCommand::FindGame);
                }
                if menu_item(ui, &netplay_voca.host_private_game) {
                    let _ = command_tx.try_send(NetplayCommand::HostGame);
                }
                if menu_item(ui, &netplay_voca.join_private_game) {
                    transition = Transition::EnterJoinCode;
                }

                actions_ui(ui, |ui| {
                    if action_button("Close").ui(ui).clicked() || esc_pressed(ui) {
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
        let mut can_retry = false;
        match netplay_connecting {
            ConnectingState::LoadingNetplayServerConfiguration => {
                title_ui(ui, "CONNECTING", MenuButton::ACTIVE_COLOR);
            }
            ConnectingState::PeeringUp(start_method, unlock_url) => {
                match start_method.as_ref() {
                    StartMethod::Start(room_name, join_or_host) => {
                        use super::connection::JoinOrHost::*;
                        title_ui(
                            ui,
                            match join_or_host {
                                Join => netplay_voca.joining_private_game.clone(),
                                Host => netplay_voca.hosting_private_game.clone(),
                            },
                            MenuButton::ACTIVE_COLOR,
                        );
                        detail_ui(ui, "WAITING FOR SECOND PLAYER");
                        title_ui(ui, "CODE", MenuButton::ACTIVE_COLOR);
                        title_ui(ui, room_name, Color32::from_rgb(255, 225, 0));
                    }
                    StartMethod::MatchWithRandom => {
                        title_ui(
                            ui,
                            netplay_voca.finding_public_game.clone(),
                            MenuButton::ACTIVE_COLOR,
                        );
                        detail_ui(ui, "WAITING FOR SECOND PLAYER");
                    }
                    StartMethod::Resume(..) => {
                        //This is used internally during the `NetplayState::Resuming` state
                    }
                }
                if let Some(unlock_url) = unlock_url {
                    Self::unlock_prompt_ui(ui, unlock_url);
                    can_retry = true;
                }
            }
        }

        actions_ui(ui, |ui| {
            if can_retry {
                self.retry_button_ui(ui);
            }
            if action_button("Cancel").ui(ui).clicked() || esc_pressed(ui) {
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
        let mut can_retry = false;
        match netplay_connected {
            SharedNetplayConnectedState::Running(start_time, connection_path, _) => {
                // Hide menu if we just managed to connect
                if Instant::now().duration_since(*start_time).as_millis() < 200 {
                    nav = Some(MainMenuState::Closed);
                }

                title_ui(ui, "CONNECTED!", MenuButton::ACTIVE_COLOR);
                if let Some(path) = connection_path {
                    detail_ui(
                        ui,
                        match path {
                            ConnectionPath::Direct => "DIRECT CONNECTION (PEER TO PEER)",
                            ConnectionPath::Relayed => "RELAYED THROUGH A TURN SERVER",
                        },
                    );
                }
            }
            SharedNetplayConnectedState::Synchronizing(start_time, unlock_url) => {
                title_ui(ui, "PAIRING UP", MenuButton::ACTIVE_COLOR);
                if let Some(unlock_url) = Self::needs_unlocking(*start_time, unlock_url) {
                    Self::unlock_prompt_ui(ui, &unlock_url);
                    can_retry = true;
                }
            }
        }
        actions_ui(ui, |ui| {
            if can_retry {
                self.retry_button_ui(ui);
            }
            if action_button("Disconnect").ui(ui).clicked() {
                let _ = netplay_tx.try_send(NetplayCommand::Disconnect);
            }
        });

        if esc_pressed(ui) {
            nav = Some(MainMenuState::Main);
        }
        nav
    }

    pub fn name(&self) -> Option<&str> {
        Some(&Bundle::current().config.vocabulary.netplay.name)
    }
}
