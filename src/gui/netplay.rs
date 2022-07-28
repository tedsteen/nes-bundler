use egui::{
    plot::{Corner, Legend},
    Button, Context, TextEdit, TextStyle, Ui, Window,
};

use crate::{
    netplay::{NetplayState, NetplayStats, NetplayServerConfiguration},
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
            use egui::plot::{Line, Plot, Value, Values};

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
                        Line::new(Values::from_values_iter(stats.get_ping().iter().map(|i| {
                            Value::new(i.duration.as_millis() as u32, i.stat.ping as f32)
                        })))
                        .name("Ping"),
                    );

                    plot_ui.line(
                        Line::new(Values::from_values_iter(stats.get_ping().iter().map(|i| {
                            Value::new(
                                i.duration.as_millis() as u32,
                                i.stat.local_frames_behind as f32,
                            )
                        })))
                        .name("Behind (local)"),
                    );

                    plot_ui.line(
                        Line::new(Values::from_values_iter(stats.get_ping().iter().map(|i| {
                            Value::new(
                                i.duration.as_millis() as u32,
                                i.stat.remote_frames_behind as f32,
                            )
                        })))
                        .name("Behind (remote)"),
                    );
                });
        }
    }
}

impl GuiComponent for NetplayGui {
    fn handle_event(&mut self, _event: &winit::event::WindowEvent, _: &mut GameRunner) {}
    fn ui(&mut self, ctx: &Context, game_runner: &mut GameRunner) {
        Window::new(self.name())
            .open(&mut self.is_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let netplay = &mut game_runner.netplay;
                ui.label(format!("Netplay id: {}", netplay.get_netplay_id(&mut game_runner.settings)));
                match &mut netplay.state {
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
                                    .on_disabled_hover_text(
                                        "Which room do you want to join?",
                                    )
                                    .clicked()
                                {
                                    netplay.connect(&netplay.room_name.clone());
                                }
                                ui.end_row();
                                ui.label("or simply");
                                if ui.button("Match with a random player").clicked() {
                                    netplay.connect("beta-0?next=2");
                                }
                                ui.end_row();

                                let NetplayServerConfiguration::Static(config) = &mut netplay.config.server;
                                ui.label("Max prediction (frames)");
                                ui.add(
                                    egui::DragValue::new(&mut config.ggrs.max_prediction)
                                        .speed(1.0)
                                        .clamp_range(1..=60),
                                );
                                ui.end_row();
                                ui.label("Input delay (frames)");
                                ui.add(
                                    egui::DragValue::new(&mut config.ggrs.input_delay)
                                        .speed(1.0)
                                        .clamp_range(1..=10),
                                );
                                ui.end_row();
                            });
                    }
                    NetplayState::Connecting(connecting_state) => {
                        if let Some(socket) = &connecting_state.socket {
                            let connected_peers = socket.connected_peers().len();
                            let remaining = MAX_PLAYERS - (connected_peers + 1);
                            ui.label(format!("Waiting for {} players", remaining));
                            if ui.button("Cancel").clicked() {
                                netplay.state = NetplayState::Disconnected;
                            }
                        }
                    }
                    NetplayState::Connected(netplay_session) => {
                        ui.collapsing("Stats", |ui| {
                            Self::stats_ui(ui, &netplay_session.stats[0], 0);
                            Self::stats_ui(ui, &netplay_session.stats[1], 1);
                        });
                        if ui.button("Disconnect").clicked() {
                            netplay.state = NetplayState::Disconnected;
                        }
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
        Self { is_open: false }
    }
}
