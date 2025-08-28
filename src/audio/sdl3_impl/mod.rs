use sdl3::AudioSubsystem;
use sdl3::audio::{
    AudioCallback, AudioDevice, AudioDeviceID, AudioFormat, AudioSpec, AudioStream as AudioStream2,
    AudioStreamWithCallback,
};

use crate::audio::NesBundlerAudioCallback;
use crate::emulation::{NESBuffers, NesStateHandler};
use crate::settings::Settings;

#[derive(Debug, Clone)]
pub struct SDL3AvailableAudioDevice {
    audio_device_id: AudioDeviceID,
}
impl SDL3AvailableAudioDevice {
    fn new(audio_device_id: AudioDeviceID) -> Self {
        Self { audio_device_id }
    }

    pub fn name(&self) -> String {
        match self.audio_device_id.id() {
            sdl3_sys::audio::SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK => {
                "Systems default speaker".to_string()
            }
            _ => self
                .audio_device_id
                .name()
                .inspect_err(|e| log::warn!("Could not get name of output device {e:?}"))
                .unwrap_or(format!("No name ({:})", self.audio_device_id.id())),
        }
    }
}

#[derive(Clone)]
pub struct SDL3AudioSystem {
    audio_subsystem: AudioSubsystem,
}

impl SDL3AudioSystem {
    pub fn new(audio_subsystem: AudioSubsystem) -> Self {
        Self { audio_subsystem }
    }
    pub fn get_available_devices(&self) -> Vec<SDL3AvailableAudioDevice> {
        let mut devices = Vec::new();

        let default =
            SDL3AvailableAudioDevice::new(self.audio_subsystem.default_playback_device().id());
        devices.push(default.clone());

        if let Ok(others) = self
            .audio_subsystem
            .audio_playback_device_ids()
            .map_err(|e| format!("TODO: Could not query for audio devices ({e:?})"))
        {
            devices.extend(
                others
                    .into_iter()
                    .filter(|id| *id != default.audio_device_id)
                    .map(SDL3AvailableAudioDevice::new),
            );
        }

        devices
    }

    pub fn get_default_device(&self) -> SDL3AvailableAudioDevice {
        self.get_available_devices()
            .first()
            .expect("At least a default device")
            .clone()
    }

    pub fn start_stream(
        &self,
        device: SDL3AvailableAudioDevice,
        audio_callback: NesBundlerAudioCallback,
    ) -> SDL3AudioStream {
        let desired_spec = AudioSpec {
            freq: Some(44100),
            channels: Some(1),
            format: Some(AudioFormat::f32_sys()),
        };

        // TODO: Check if 735 is right for all systems (Pal, Ntsc, Dendy etc..)
        sdl3::hint::set("SDL_AUDIO_DEVICE_SAMPLE_FRAMES", "735");
        let stream = AudioDevice::open_playback(
            &self.audio_subsystem,
            Some(&device.audio_device_id),
            &desired_spec,
        )
        .map_err(anyhow::Error::msg)
        .expect("TODO")
        .open_playback_stream_with_callback(&desired_spec, audio_callback)
        .expect("TODO");

        stream.resume().expect("TODO");
        SDL3AudioStream {
            audio_stream_with_callback: stream,
        }
        //std::thread::sleep(Duration::from_secs(3));
    }
}

pub struct SDL3AudioStream {
    pub audio_stream_with_callback: AudioStreamWithCallback<NesBundlerAudioCallback>,
}

impl AudioCallback<f32> for NesBundlerAudioCallback {
    fn callback(&mut self, stream: &mut AudioStream2, requested: i32) {
        match self.nes_state.try_lock() {
            Ok(mut nes_state) => {
                // Push at least as much as is requested
                while (self.audio_buffer.len() as i32) < requested {
                    nes_state.advance(
                        *self.inputs.read().unwrap(),
                        &mut NESBuffers {
                            audio: Some(&mut self.audio_buffer),
                            video: self.frame_buffer.push_ref().as_deref_mut().ok(),
                        },
                    );
                }
            }
            Err(e) => {
                println!("Failed to get lock for nes_state in audio callback! {e:?}");
            }
        }

        // println!(
        //     "Requested: {requested:?}, Produced: {:?}",
        //     self.audio_buffer.len()
        // );
        let volume = match Settings::try_current() {
            Ok(settings) => settings.audio.volume as f32 / 100.0,
            Err(e) => {
                println!("Failed to get lock for settings in audio callback! {e:?}");
                0.0
            }
        };
        for s in &mut self.audio_buffer {
            *s *= volume;
        }

        stream.put_data_f32(&self.audio_buffer).unwrap();
        self.audio_buffer.clear();
    }
}
