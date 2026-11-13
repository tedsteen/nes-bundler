use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::mpsc::{Receiver, Sender};

use sdl3::AudioSubsystem;
use sdl3::audio::{
    AudioCallback, AudioDevice, AudioDeviceID, AudioFormat, AudioSpec, AudioStream as AudioStream2,
    AudioStreamWithCallback,
};
use sdl3::sys::audio::SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK;

use crate::audio::buffer::{AudioBufferControl, AudioConsumer, AudioProducer, audio_buffer};
use crate::audio::{AudioStream, AudioSystem, AvailableAudioDevice};
use crate::emulation::DEFAULT_SAMPLE_RATE;
use crate::settings::SettingsStore;

// How many samples the sound card takes at a time. Smaller means less audio waiting in the device
// and less headroom kept in the buffer. On Linux, PipeWire shrinks the whole graph's quantum to
// match, and ~1 ms (45 samples) caused constant xruns on modest hardware, so it gets 256 (~6 ms);
// elsewhere 128 (~3 ms) is routine.
#[cfg(target_os = "linux")]
const CALLBACK_FRAMES: u32 = 256;
#[cfg(not(target_os = "linux"))]
const CALLBACK_FRAMES: u32 = 128;

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
            SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK => "Systems default speaker".to_string(),
            _ => self
                .audio_device_id
                .name()
                .inspect_err(|e| log::warn!("Could not get name of output device {e:?}"))
                .unwrap_or_else(|_| format!("No name ({})", self.audio_device_id.id().value())),
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

        match self.audio_subsystem.audio_playback_device_ids() {
            Ok(ids) => devices.extend(
                ids.into_iter()
                    .filter(|id| *id != default.audio_device_id)
                    .map(SDL3AvailableAudioDevice::new),
            ),
            Err(e) => log::error!("Could not list audio devices: {e}"),
        }

        devices
    }

    pub fn get_default_device(&self) -> SDL3AvailableAudioDevice {
        SDL3AvailableAudioDevice::new(self.audio_subsystem.default_playback_device().id())
    }

    pub fn start_stream(&self, settings_store: &SettingsStore) -> AudioStream {
        let mut settings = settings_store.write();
        let device = settings.audio.resolve_output_device(self);
        settings.audio.sync_output_device_selection(&device);
        let volume = settings.audio.volume;
        let latency_micros = settings.audio.latency_micros;
        AudioStream::new(self.clone(), device, volume, latency_micros)
    }
}

pub struct SDL3AudioStream {
    audio_system: AudioSystem,
    audio_stream_with_callback: Option<AudioStreamWithCallback<NesBundlerAudioCallback>>,
    tx: Option<AudioProducer>,
    take_back: Receiver<AudioConsumer>,
    volume: Arc<AtomicU8>,
    ctl: AudioBufferControl,
}

impl SDL3AudioStream {
    pub fn take_producer(&mut self) -> AudioProducer {
        self.tx.take().expect("AudioProducer already taken")
    }
}

impl SDL3AudioStream {
    fn new(
        audio_system: AudioSystem,
        device: AvailableAudioDevice,
        volume: u8,
        latency_micros: u32,
    ) -> Self {
        let (producer, consumer, ctl) = audio_buffer(
            latency_micros,
            DEFAULT_SAMPLE_RATE,
            CALLBACK_FRAMES as usize,
        );

        let volume = Arc::new(AtomicU8::new(volume));
        let mut stream = Self {
            audio_stream_with_callback: None,
            take_back: std::sync::mpsc::channel().1,
            audio_system,
            tx: Some(producer),
            volume,
            ctl,
        };
        stream.open(device, consumer);
        stream
    }

    /// Plays through `device`, or the default device should it fail to open, or nothing should
    /// that fail too: no sound beats no game.
    fn open(&mut self, device: AvailableAudioDevice, consumer: AudioConsumer) {
        let default = self.audio_system.get_default_device();
        let mut consumer = consumer;
        for device in [device.clone(), default.clone()] {
            let (stream, take_back) = Self::create(
                self.audio_system.audio_subsystem.clone(),
                &device,
                consumer,
                self.volume.clone(),
            );
            self.take_back = take_back;
            match stream {
                Ok(stream) => {
                    self.audio_stream_with_callback = Some(stream);
                    return;
                }
                Err(e) => {
                    log::error!("Could not play audio through {}: {e:#}", device.name());
                    if device.audio_device_id == default.audio_device_id {
                        break;
                    }
                    consumer = self.take_consumer();
                }
            }
        }
        log::error!("No audio device could be opened, playing without sound");
    }

    /// The consumer back from the stream that had it, once that stream is dropped (or failed to
    /// open).
    fn take_consumer(&self) -> AudioConsumer {
        self.take_back
            .recv()
            .expect("the audio consumer back from the stream")
    }

    fn create(
        audio_subsystem: AudioSubsystem,
        device: &AvailableAudioDevice,
        rx: AudioConsumer,
        volume: Arc<AtomicU8>,
    ) -> (
        anyhow::Result<AudioStreamWithCallback<NesBundlerAudioCallback>>,
        Receiver<AudioConsumer>,
    ) {
        let desired_spec = AudioSpec {
            freq: Some(DEFAULT_SAMPLE_RATE as i32),
            channels: Some(1),
            format: Some(AudioFormat::f32_sys()),
        };
        let (give_back, take_back) = std::sync::mpsc::channel();
        // Gives the consumer back when dropped, also when the stream fails to open.
        let callback = NesBundlerAudioCallback {
            tmp: Vec::new(),
            rx: Some(rx),
            give_back,
            volume,
        };

        sdl3::hint::set(
            "SDL_AUDIO_DEVICE_SAMPLE_FRAMES",
            &CALLBACK_FRAMES.to_string(),
        );

        let stream = AudioDevice::open_playback(
            &audio_subsystem,
            Some(&device.audio_device_id),
            &desired_spec,
        )
        .map_err(anyhow::Error::msg)
        .and_then(|audio_device| {
            audio_device
                .open_playback_stream_with_callback(&desired_spec, callback)
                .map_err(anyhow::Error::msg)
        })
        .and_then(|stream| {
            stream.resume().map_err(anyhow::Error::msg)?;
            Ok(stream)
        });
        (stream, take_back)
    }

    pub(crate) fn swap_output_device(&mut self, device: AvailableAudioDevice) {
        if let Some(stream) = self.audio_stream_with_callback.take() {
            let _ = stream.pause();
        }
        let consumer = self.take_consumer();
        self.open(device, consumer);
    }

    pub(crate) fn set_volume(&mut self, volume: u8) {
        self.volume
            .store(volume, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn set_latency(&self, latency_micros: u32) {
        self.ctl.set_latency_micros(latency_micros);
    }

    /// How long ago the sound last cut out while stuttering, if it has.
    pub(crate) fn since_stutter(&self) -> Option<std::time::Duration> {
        self.ctl.since_stutter()
    }
}

pub struct NesBundlerAudioCallback {
    tmp: Vec<f32>,
    rx: Option<AudioConsumer>,
    give_back: Sender<AudioConsumer>,
    volume: Arc<AtomicU8>,
}

impl AudioCallback<f32> for NesBundlerAudioCallback {
    fn callback(&mut self, stream: &mut AudioStream2, requested: i32) {
        let requested = requested as usize;

        if requested > self.tmp.len() {
            // Amortized growth (pow2). Avoids frequent reallocs.
            let new_len = requested.next_power_of_two();
            self.tmp.resize(new_len, 0.0);
        }

        if let Some(rx) = &mut self.rx {
            let buf = &mut self.tmp[..requested];
            // A fade to silence fills the rest when the buffer can't cover it.
            rx.pop_slice(buf);

            let gain = (self.volume.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
            for x in buf {
                *x *= gain;
            }
        } else {
            self.tmp[..requested].fill(0.0);
        }
        let _ = stream.put_data_f32(&self.tmp[..requested]);
    }
}

impl Drop for NesBundlerAudioCallback {
    fn drop(&mut self) {
        if let Some(rx) = self.rx.take() {
            // Nobody to give it back to when the app is shutting down.
            let _ = self.give_back.send(rx);
        }
    }
}
