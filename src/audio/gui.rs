use crate::{
    audio::{AudioStream, AudioSystem, MAX_AUDIO_LATENCY_MICROS, MIN_AUDIO_LATENCY_MICROS},
    main_view::gui::GuiComponent,
    settings::Settings,
};
use egui::{Slider, Ui};

pub struct AudioGui {
    pub audio_system: AudioSystem,
    audio_stream: AudioStream,
    // #[cfg(feature = "debug")]
    // stats: AudioStats,
}

impl AudioGui {
    pub fn new(audio_system: AudioSystem, audio_stream: AudioStream) -> Self {
        Self {
            audio_system,
            //stats: AudioStats::new(),
            audio_stream,
        }
    }
}
// #[cfg(feature = "debug")]
// impl AudioGui {
//     fn stats_ui(ui: &mut egui::Ui, stats: &AudioStats) {
//         use egui_plot::{Line, Plot};

//         Plot::new("stats_plot_audio_stats".to_string())
//             .label_formatter(|name, value| {
//                 if !name.is_empty() {
//                     format!("{name}: {}", value.y)
//                 } else {
//                     "".to_string()
//                 }
//             })
//             .legend(
//                 egui_plot::Legend::default()
//                     .position(egui_plot::Corner::LeftTop)
//                     .text_style(egui::TextStyle::Small),
//             )
//             .view_aspect(2.0)
//             .include_y(0)
//             .show_axes([false, true])
//             .show(ui, |plot_ui| {
//                 plot_ui.line(
//                     Line::new(
//                         stats
//                             .stats
//                             .iter()
//                             .enumerate()
//                             .map(|(idx, i)| [idx as f64, i.latency as f64])
//                             .collect::<egui_plot::PlotPoints>(),
//                     )
//                     .name("Ping"),
//                 );
//             });
//     }
// }

impl GuiComponent for AudioGui {
    fn ui(&mut self, ui: &mut Ui) {
        // #[cfg(feature = "debug")]
        // Self::stats_ui(ui, &self.stats);
        let available_devices = self.audio_system.get_available_devices();
        let audio_settings = &mut Settings::current_mut().audio;
        ui.horizontal(|ui| {
            ui.label("Output");
            let selected_device = &mut audio_settings.output_device;
            if selected_device.is_none() {
                *selected_device = Some(self.audio_system.get_default_device().name())
            }
            if let Some(selected_text) = selected_device.as_deref_mut() {
                egui::ComboBox::from_id_salt("audio-output")
                    .width(160.0)
                    .selected_text(selected_text.to_string())
                    .show_ui(ui, |ui| {
                        for available_device in available_devices {
                            let a = ui.selectable_value(
                                selected_device,
                                Some(available_device.name()),
                                available_device.name(),
                            );
                            if a.changed() {
                                self.audio_stream.swap_output_device(available_device);
                            }
                        }
                    });
            }
        });

        ui.horizontal(|ui| {
            ui.label("Volume");
            if ui
                .add(Slider::new(&mut audio_settings.volume, 0..=100).suffix("%"))
                .changed()
            {
                self.audio_stream.set_volume(audio_settings.volume);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Latency");
            if ui
                .add(
                    Slider::new(
                        &mut audio_settings.latency_micros,
                        MIN_AUDIO_LATENCY_MICROS..=MAX_AUDIO_LATENCY_MICROS,
                    )
                    .suffix("ms")
                    .custom_formatter(|ns, _| format!("{}", ns / 1000.0))
                    .logarithmic(true),
                )
                .changed()
            {
                self.audio_stream.set_latency(audio_settings.latency_micros);
            }
        });
    }

    fn name(&self) -> Option<&str> {
        Some("Audio")
    }
}
