use crate::{main_view::gui::GuiComponent, settings::Settings};
use egui::{Slider, Ui};

use super::{
    //debug::{AudioStat, AudioStats},
    Audio,
};

pub struct AudioGui {
    pub audio: Audio,
    // #[cfg(feature = "debug")]
    // stats: AudioStats,
}

impl AudioGui {
    pub fn new(audio: Audio) -> Self {
        Self {
            audio,
            //stats: AudioStats::new(),
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
        let available_device_names =
            Audio::get_available_output_device_names_for_subsystem(&self.audio.audio_subsystem);
        let new_device = {
            let mut new_device = None;
            let audio_settings = &mut Settings::current_mut().audio;
            ui.horizontal(|ui| {
                ui.label("Output");
                let selected_device = &mut audio_settings.output_device;
                if selected_device.is_none() {
                    *selected_device = self.audio.get_default_device_name();
                }
                if let Some(selected_text) = selected_device.as_deref_mut() {
                    egui::ComboBox::from_id_salt("audio-output")
                        .width(160.0)
                        .selected_text(selected_text.to_string())
                        .show_ui(ui, |ui| {
                            for name in available_device_names {
                                if ui
                                    .selectable_value(
                                        selected_device,
                                        Some(name.clone()),
                                        name.clone(),
                                    )
                                    .changed()
                                {
                                    new_device = Some(name);
                                }
                            }
                        });
                }
            });

            ui.horizontal(|ui| {
                ui.label("Volume");
                ui.add(Slider::new(&mut audio_settings.volume, 0..=100).suffix("%"));
            });

            new_device
        };
        if let Some(new_device) = new_device {
            self.audio.stream.set_output_device(Some(new_device));
        }
    }

    fn name(&self) -> Option<&str> {
        Some("Audio")
    }
}
