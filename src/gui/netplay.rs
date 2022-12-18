use std::time::{Duration, Instant};

use egui::{
    plot::{Corner, Legend, PlotPoints},
    Button, Context, TextEdit, TextStyle, Ui, Window,
};

use crate::{
    netplay::{
        state::{ConnectedState, ConnectingState, PeeringState, StartMethod},
        NetplayState, NetplayStats,
    },
    settings::MAX_PLAYERS,
    GameRunner,
};

use super::GuiComponent;
pub struct NetplayGui {
    is_open: bool,
}
impl NetplayGui {
    fn stats_ui(ui: &mut Ui, stats: &NetplayStats, player: usize) {
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
                    Legend::default()
                        .position(Corner::LeftTop)
                        .text_style(TextStyle::Small),
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
                                .collect::<PlotPoints>(),
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
                                .collect::<PlotPoints>(),
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
                                .collect::<PlotPoints>(),
                        )
                        .name("Behind (remote)"),
                    );
                });
        }
    }
}

impl GuiComponent for NetplayGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _: &mut GameRunner) {}
    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner, ui_visible: bool) {
        let netplay = &mut game_runner.netplay;
        if let NetplayState::Connected(_netplay_session, ConnectedState::MappingInput) =
            &mut netplay.state
        {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Floating text!");
            });
        }
        if !ui_visible {
            return;
        }
        Window::new(self.name())
            .open(&mut self.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| match &netplay.state {
                NetplayState::Disconnected => {
                    egui::Grid::new("netplay_grid")
                        .num_columns(2)
                        .spacing([10.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("Join a game by name");
                            ui.add(
                                TextEdit::singleline(&mut netplay.room_name)
                                    .desired_width(140.0)
                                    .hint_text("Netplay room"),
                            );
                            if ui
                                .add_enabled(!netplay.room_name.is_empty(), Button::new("Join"))
                                .on_disabled_hover_text("Which room do you want to join?")
                                .clicked()
                            {
                                netplay.start(StartMethod::Create(netplay.room_name.clone()));
                            }
                            ui.end_row();
                            ui.label("or simply");
                            if ui.button("Match with a random player").clicked() {
                                netplay.start(StartMethod::Random);
                            }
                            ui.end_row();
                        });
                }
                NetplayState::Connecting(start_method, connecting_state) => {
                    match connecting_state {
                        ConnectingState::LoadingNetplayServerConfiguration(_) => {
                            ui.label("Initializing");
                            if ui.button("Cancel").clicked() {
                                netplay.state = NetplayState::Disconnected;
                            }
                        }
                        ConnectingState::PeeringUp(PeeringState { socket, .. }) => {
                            if let Some(socket) = socket {
                                let connected_peers = socket.connected_peers().len();
                                let remaining = MAX_PLAYERS - (connected_peers + 1);
                                ui.label(format!("Waiting for {} players...", remaining));
                                if ui.button("Cancel").clicked() {
                                    netplay.state = NetplayState::Disconnected;
                                }
                            }
                        }
                        ConnectingState::Synchronizing(synchronizing_state) => {
                            ui.label("Synchronizing players...");
                            if let Some(unlock_url) = &synchronizing_state.unlock_url {
                                if Instant::now()
                                    .duration_since(synchronizing_state.start_time)
                                    .gt(&Duration::from_secs(5))
                                {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        ui.label("We're having trouble connecting you, click ");
                                        ui.hyperlink_to("here", unlock_url);
                                        ui.label(" to unlock Netplay!");
                                    });
                                    if ui.button("Retry").clicked() {
                                        netplay.start(start_method.clone());
                                    }
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                netplay.state = NetplayState::Disconnected;
                            }
                        }
                    }
                }
                NetplayState::Connected(netplay_session, _) => {
                    ui.collapsing("Stats", |ui| {
                        Self::stats_ui(ui, &netplay_session.stats[0], 0);
                        Self::stats_ui(ui, &netplay_session.stats[1], 1);
                    });
                    if ui.button("Disconnect").clicked() {
                        netplay.state = NetplayState::Disconnected;
                    }
                }
            });
    }

    fn is_open(&mut self) -> &mut bool {
        &mut self.is_open
    }

    fn name(&self) -> String {
        "Netplay!".to_string()
    }
}

impl NetplayGui {
    pub fn new() -> Self {
        Self { is_open: true }
    }
}
