use crate::{
    audio::{AudioStream, AudioSystem, MAX_AUDIO_LATENCY_MICROS, MIN_AUDIO_LATENCY_MICROS},
    main_view::gui::{GuiComponent, MainMenuState},
    settings::SettingsStore,
};
use egui::{Slider, Ui};

pub struct AudioGui {
    pub audio_system: AudioSystem,
    audio_stream: AudioStream,
    settings: &'static SettingsStore,
}

impl AudioGui {
    pub fn new(
        audio_system: AudioSystem,
        audio_stream: AudioStream,
        settings: &'static SettingsStore,
    ) -> Self {
        Self {
            audio_system,
            audio_stream,
            settings,
        }
    }
}

impl GuiComponent for AudioGui {
    fn ui(&mut self, ui: &mut Ui) -> Option<MainMenuState> {
        let available_devices = self.audio_system.get_available_devices();
        let mut settings = self.settings.write();
        let audio_settings = &mut settings.audio;
        ui.horizontal(|ui| {
            ui.label("Output");
            let selected_device = &mut audio_settings.output_device;
            if selected_device.is_none() {
                *selected_device = Some(self.audio_system.get_default_device().name())
            }
            if let Some(selected_text) = selected_device.as_deref() {
                egui::ComboBox::from_id_salt("audio-output")
                    .width(160.0)
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for available_device in available_devices {
                            let name = available_device.name();
                            let a = ui.selectable_value(selected_device, Some(name.clone()), name);
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
        None
    }

    fn name(&self) -> Option<&str> {
        Some("Audio")
    }
}
