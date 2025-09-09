use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::mpsc::{Receiver, Sender};

use ringbuf::traits::Consumer;
use sdl3::AudioSubsystem;
use sdl3::audio::{
    AudioCallback, AudioDevice, AudioDeviceID, AudioFormat, AudioSpec, AudioStream as AudioStream2,
    AudioStreamWithCallback,
};
use sdl3::sys::audio::SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK;

use crate::audio::pacer::{AudioConsumer, AudioProducer, make_paced_bridge_ringbuf_bulk_async};
use crate::audio::{AudioStream, AudioSystem, AvailableAudioDevice};
use crate::emulation::DEFAULT_SAMPLE_RATE;

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

        devices.extend(
            self.audio_subsystem
                .audio_playback_device_ids()
                .map_err(|e| format!("Could not query for audio devices ({e:?})"))
                .unwrap()
                .into_iter()
                .filter(|id| *id != default.audio_device_id)
                .map(SDL3AvailableAudioDevice::new),
        );

        devices
    }

    pub fn get_default_device(&self) -> SDL3AvailableAudioDevice {
        self.get_available_devices()
            .first()
            .expect("At least a default device")
            .clone()
    }

    pub fn start_stream(&self, device: AvailableAudioDevice, volume: u8) -> AudioStream {
        AudioStream::new(self.clone(), device, volume)
    }
}

pub struct SDL3AudioStream {
    audio_system: AudioSystem,
    audio_stream_with_callback: Option<AudioStreamWithCallback<NesBundlerAudioCallback>>,
    pub tx: Option<AudioProducer>,
    take_back: Receiver<AudioConsumer>,
    volume: Arc<AtomicU8>,
    _c: super::pacer::BridgeGuard,
}

impl SDL3AudioStream {
    fn new(audio_system: AudioSystem, device: AvailableAudioDevice, volume: u8) -> Self {
        let (tx, rx, c) = make_paced_bridge_ringbuf_bulk_async(25.0, DEFAULT_SAMPLE_RATE as f64);

        let volume = Arc::new(AtomicU8::new(volume));
        let (stream, take_back) = Self::create(
            audio_system.audio_subsystem.clone(),
            device,
            rx,
            volume.clone(),
        );
        Self {
            audio_stream_with_callback: Some(stream),
            audio_system,
            tx: Some(tx),
            take_back,
            volume,
            _c: c,
        }
    }
    fn create(
        audio_subsystem: AudioSubsystem,
        device: AvailableAudioDevice,
        rx: AudioConsumer,
        volume: Arc<AtomicU8>,
    ) -> (
        AudioStreamWithCallback<NesBundlerAudioCallback>,
        Receiver<AudioConsumer>,
    ) {
        let desired_spec = AudioSpec {
            freq: Some(DEFAULT_SAMPLE_RATE as i32),
            channels: Some(1),
            format: Some(AudioFormat::f32_sys()),
        };
        let (give_back, take_back) = std::sync::mpsc::channel();

        // TODO: Calculate the correct buffer hint here
        sdl3::hint::set("SDL_AUDIO_DEVICE_SAMPLE_FRAMES", "735");

        let stream = AudioDevice::open_playback(
            &audio_subsystem,
            Some(&device.audio_device_id),
            &desired_spec,
        )
        .map_err(anyhow::Error::msg)
        .expect("An audio device")
        .open_playback_stream_with_callback(
            &desired_spec,
            NesBundlerAudioCallback {
                tmp: Vec::new(),
                rx: Some(rx),
                give_back,
                volume,
            },
        )
        .expect("The stream to start");

        stream.resume().expect("The stream to resume");
        (stream, take_back)
    }

    pub(crate) fn swap_output_device(&mut self, device: AvailableAudioDevice) {
        if let Some(stream) = self.audio_stream_with_callback.take() {
            stream.pause().expect("The stream to pause");
            drop(stream);
            let (stream, take_back) = Self::create(
                self.audio_system.audio_subsystem.clone(),
                device,
                self.take_back.recv().unwrap(),
                self.volume.clone(),
            );
            self.take_back = take_back;

            self.audio_stream_with_callback = Some(stream);
        }
    }

    pub(crate) fn set_volume(&mut self, volume: u8) {
        self.volume
            .store(volume, std::sync::atomic::Ordering::Relaxed);
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
            let got = rx.pop_slice(buf);

            // Apply volume
            let gain = (self.volume.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
            for x in &mut buf[..got] {
                *x *= gain;
            }

            if got < buf.len() {
                buf[got..].fill(0.0);
            }

            // zero-pad if we under-ran so we still hand over 'requested' frames
            if got < requested {
                //log::warn!("Buffer underrun ({got} < {requested})");
                buf[got..requested].fill(0.0);
            }
        } else {
            self.tmp[..requested].fill(0.0);
        }
        let _ = stream.put_data_f32(&self.tmp[..requested]);
    }
}

impl Drop for NesBundlerAudioCallback {
    fn drop(&mut self) {
        self.give_back
            .send(self.rx.take().expect("an audio consumer"))
            .expect("To be able to give back the audio consumer");
    }
}
