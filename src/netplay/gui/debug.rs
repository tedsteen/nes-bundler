use super::NetplayGui;

impl NetplayGui {
    #[cfg(feature = "debug")]
    pub(crate) fn stats_ui(
        ui: &mut egui::Ui,
        stats: &crate::netplay::stats::NetplayStats,
        player: usize,
    ) {
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
                    plot_ui.line(Line::new(
                        "Ping",
                        stats
                            .get_ping()
                            .iter()
                            .map(|i| [i.duration.as_millis() as f64, i.stat.ping as f64])
                            .collect::<egui_plot::PlotPoints>(),
                    ));

                    plot_ui.line(Line::new(
                        "Behind (local)",
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
                    ));

                    plot_ui.line(Line::new(
                        "Behind (remote)",
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
                    ));
                });
        }
    }
}
