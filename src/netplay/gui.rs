use std::time::{Duration, Instant};

use egui::{Button, Context, TextEdit, Window};

use crate::settings::{gui::GuiComponent, MAX_PLAYERS};

use super::{
    connecting::{Connecting, PeeringState},
    netplay_state_machine::NetplayState,
    state_handler::NetplayStateHandler,
    ConnectingState, StartMethod,
};

#[cfg(feature = "debug")]
impl NetplayGui {
    fn stats_ui(ui: &mut egui::Ui, stats: &super::stats::NetplayStats, player: usize) {
        if !stats.get_ping().is_empty() {
            ui.label(format!("Player {player}"));
            use egui::plot::{Line, Plot};

            Plot::new(format!("stats_plot_{player}"))
                .label_formatter(|name, value| {
                    if !name.is_empty() {
                        format!("{name}: {}", value.y)
                    } else {
                        "".to_string()
                    }
                })
                .legend(
                    egui::plot::Legend::default()
                        .position(egui::plot::Corner::LeftTop)
                        .text_style(egui::TextStyle::Small),
                )
                .view_aspect(2.0)
                .include_y(0)
                .show_axes([false, true])
                .show(ui, |plot_ui| {
                    plot_ui.line(
                        Line::new(
                            stats
                                .get_ping()
                                .iter()
                                .map(|i| [i.duration.as_millis() as f64, i.stat.ping as f64])
                                .collect::<egui::plot::PlotPoints>(),
                        )
                        .name("Ping"),
                    );

                    plot_ui.line(
                        Line::new(
                            stats
                                .get_ping()
                                .iter()
                                .map(|i| {
                                    [
                                        i.duration.as_millis() as f64,
                                        i.stat.local_frames_behind as f64,
                                    ]
                                })
                                .collect::<egui::plot::PlotPoints>(),
                        )
                        .name("Behind (local)"),
                    );

                    plot_ui.line(
                        Line::new(
                            stats
                                .get_ping()
                                .iter()
                                .map(|i| {
                                    [
                                        i.duration.as_millis() as f64,
                                        i.stat.remote_frames_behind as f64,
                                    ]
                                })
                                .collect::<egui::plot::PlotPoints>(),
                        )
                        .name("Behind (remote)"),
                    );
                });
        }
    }
}
pub struct NetplayGui {
    room_name: String,
    is_open: bool,
}

impl NetplayGui {
    pub fn new(room_name: String) -> Self {
        Self {
            room_name,
            is_open: true,
        }
    }
}

impl GuiComponent for NetplayStateHandler {
    fn ui(&mut self, ctx: &Context, ui_visible: bool, name: String) {
        if let NetplayState::Connecting(_) | NetplayState::Resuming(_) =
            &mut self.netplay.as_mut().unwrap()
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if let NetplayState::Resuming(_) = &mut self.netplay.as_mut().unwrap()
                    {
                        ui.label("Connection lost, trying to reconnect...\nSee NetPlay! settings for details");    
                    } else {
                        ui.label("Connecting...\nSee NetPlay! settings for details");
                    }
                });
            });
        }
        if !ui_visible {
            return;
        }

        Window::new(name)
            .open(&mut self.gui.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                self.netplay = Some(match self.netplay.take().unwrap() {
                    NetplayState::Disconnected(netplay_disconnected) => {
                        let mut join_clicked = false;
                        let mut random_clicked = false;

                        egui::Grid::new("netplay_grid")
                            .num_columns(2)
                            .spacing([10.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Join a game by name");
                                ui.add(
                                    TextEdit::singleline(&mut self.gui.room_name)
                                        .desired_width(140.0)
                                        .hint_text("Netplay room"),
                                );
                                join_clicked = ui
                                    .add_enabled(
                                        !self.gui.room_name.is_empty(),
                                        Button::new("Join"),
                                    )
                                    .on_disabled_hover_text("Which room do you want to join?")
                                    .clicked();
                                ui.end_row();
                                ui.label("or simply");
                                random_clicked = ui.button("Match with a random player").clicked();
                                ui.end_row();
                            });
                        if join_clicked {
                            NetplayState::Connecting(
                                netplay_disconnected
                                    .start(StartMethod::Create(self.gui.room_name.clone())),
                            )
                        } else if random_clicked {
                            NetplayState::Connecting(
                                netplay_disconnected.start(StartMethod::Random),
                            )
                        } else {
                            NetplayState::Disconnected(netplay_disconnected)
                        }
                    }
                    NetplayState::Resuming(netplay_resuming) => {
                        ui.label("Resuming...");
                        if ui.button("Cancel").clicked() {
                            NetplayState::Disconnected(netplay_resuming.cancel())
                        } else {
                            NetplayState::Resuming(netplay_resuming)
                        }
                    }
                    NetplayState::Connecting(netplay_connecting) => {
                        let mut retry_start_method = None;

                        #[allow(clippy::collapsible_match)]
                        match &netplay_connecting.state {
                            ConnectingState::LoadingNetplayServerConfiguration(_) => {
                                ui.label("Initializing...");
                            }

                            ConnectingState::PeeringUp(Connecting::<PeeringState> {
                                state: PeeringState { socket, .. },
                                ..
                            }) => {
                                ui.label("Peering up...");
                                if let Some(socket) = socket {
                                    let connected_peers = socket.connected_peers().count();
                                    let remaining = MAX_PLAYERS - (connected_peers + 1);
                                    ui.label(format!("Waiting for {} players...", remaining));
                                }
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
                            _ => {}
                        }
                        if let Some(start_method) = retry_start_method {
                            NetplayState::Connecting(
                                netplay_connecting.cancel().start(start_method),
                            )
                        } else if ui.button("Cancel").clicked() {
                            NetplayState::Disconnected(netplay_connecting.cancel())
                        } else {
                            NetplayState::Connecting(netplay_connecting)
                        }
                    }
                    NetplayState::Connected(netplay_connected) => {
                        #[cfg(feature = "debug")]
                        let fake_lost_connection_clicked = {
                            ui.collapsing("Stats", |ui| {
                                NetplayGui::stats_ui(
                                    ui,
                                    &netplay_connected.state.netplay_session.stats[0],
                                    0,
                                );
                                NetplayGui::stats_ui(
                                    ui,
                                    &netplay_connected.state.netplay_session.stats[1],
                                    1,
                                );
                            });
                            ui.button("Fake connection lost").clicked()
                        };

                        if ui.button("Disconnect").clicked() {
                            NetplayState::Disconnected(netplay_connected.disconnect())
                        } else if fake_lost_connection_clicked {
                            println!(
                                "Faking a lost connection. Resume frames available {:?}",
                                netplay_connected
                                    .state
                                    .netplay_session
                                    .last_confirmed_game_states
                                    .clone()
                                    .map(|s| s.frame)
                            );
                            NetplayState::Resuming(netplay_connected.resume())
                        } else {
                            NetplayState::Connected(netplay_connected)
                        }
                    }
                });
            });
    }

    fn name(&self) -> Option<String> {
        Some("Netplay!".to_string())
    }

    fn open(&mut self) -> &mut bool {
        &mut self.gui.is_open
    }

    fn event(&mut self, _event: &winit::event::Event<()>) {}
}
