use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use ggrs::NetworkStats;

use crate::{
    netplay::{netplay_state::NetplayState, NetplayStateHandler},
    settings::MAX_PLAYERS,
};

pub struct NetplayStat {
    pub stat: NetworkStats,
    pub duration: Duration,
}
pub const STATS_HISTORY: usize = 100;

pub struct NetplayStats {
    stats: VecDeque<NetplayStat>,
    start_time: Instant,
}

impl NetplayStats {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            stats: VecDeque::with_capacity(STATS_HISTORY),
        }
    }

    pub fn get_ping(&self) -> &VecDeque<NetplayStat> {
        &self.stats
    }

    pub fn push_stats(&mut self, stat: NetworkStats) {
        let duration = Instant::now().duration_since(self.start_time);
        self.stats.push_back(NetplayStat { duration, stat });
        if self.stats.len() == STATS_HISTORY {
            self.stats.pop_front();
        }
    }
}
use super::NetplayGui;

impl NetplayGui {
    pub fn prepare(&mut self, netplay_state_handler: &NetplayStateHandler) {
        if let Some(NetplayState::Connected(netplay)) = &netplay_state_handler.netplay {
            let sess = &netplay.state.netplay_session.p2p_session;
            if netplay.state.netplay_session.game_state.frame % 30 == 0 {
                for i in 0..MAX_PLAYERS {
                    if let Ok(stats) = sess.network_stats(i) {
                        if !sess.local_player_handles().contains(&i) {
                            self.stats[i].push_stats(stats);
                        }
                    }
                }
            };
        }
    }
    pub(crate) fn stats_ui(ui: &mut egui::Ui, stats: &NetplayStats, player: usize) {
        if !stats.get_ping().is_empty() {
            ui.label(format!("Player {player}"));
            use egui_plot::{Line, Plot};

            Plot::new(format!("stats_plot_{player}"))
                .label_formatter(|name, value| {
                    if !name.is_empty() {
                        format!("{name}: {}", value.y)
                    } else {
                        "".to_string()
                    }
                })
                .legend(
                    egui_plot::Legend::default()
                        .position(egui_plot::Corner::LeftTop)
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
                                .collect::<egui_plot::PlotPoints>(),
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
                                .collect::<egui_plot::PlotPoints>(),
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
                                .collect::<egui_plot::PlotPoints>(),
                        )
                        .name("Behind (remote)"),
                    );
                });
        }
    }
}
