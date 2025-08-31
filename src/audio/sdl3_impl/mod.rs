use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::sync::mpsc::{Receiver, Sender};

use ringbuf::storage::Heap;
use ringbuf::traits::{Consumer, Split};
use sdl3::AudioSubsystem;
use sdl3::audio::{
    AudioCallback, AudioDevice, AudioDeviceID, AudioFormat, AudioSpec, AudioStream as AudioStream2,
    AudioStreamWithCallback,
};

use crate::audio::{AudioProducer, AudioStream, AudioSystem, AvailableAudioDevice};

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

    pub fn start_stream(&self, device: AvailableAudioDevice, volume: u8) -> AudioStream {
        AudioStream::new(self.clone(), device, volume)
    }
}
type RingbufType = ringbuf::SharedRb<Heap<f32>>;
type AudioConsumer = ringbuf::wrap::caching::Caching<std::sync::Arc<RingbufType>, false, true>;

pub struct SDL3AudioStream {
    audio_system: AudioSystem,
    audio_stream_with_callback: Option<AudioStreamWithCallback<NesBundlerAudioCallback>>,
    pub tx: Option<AudioProducer>,
    take_back: Receiver<AudioConsumer>,
    volume: Arc<AtomicU8>,
}
impl SDL3AudioStream {
    fn new(audio_system: AudioSystem, device: AvailableAudioDevice, volume: u8) -> Self {
        let (tx, rx) = RingbufType::new(735 * 100).split();
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
            freq: Some(44100),
            channels: Some(1),
            format: Some(AudioFormat::f32_sys()),
        };
        let (give_back, take_back) = std::sync::mpsc::channel();

        // TODO: Check if 735 is right for all systems (Pal, Ntsc, Dendy etc..)
        sdl3::hint::set("SDL_AUDIO_DEVICE_SAMPLE_FRAMES", "735");

        let stream = AudioDevice::open_playback(
            &audio_subsystem,
            Some(&device.audio_device_id),
            &desired_spec,
        )
        .map_err(anyhow::Error::msg)
        .expect("TODO")
        .open_playback_stream_with_callback(
            &desired_spec,
            NesBundlerAudioCallback {
                tmp: [0_f32; AUDIO_SCRATCH_SIZE as usize],
                rx: Some(rx),
                give_back,
                volume,
            },
        )
        .expect("TODO");

        stream.resume().expect("TODO");
        (stream, take_back)
    }

    pub(crate) fn swap_output_device(&mut self, device: AvailableAudioDevice) {
        if let Some(stream) = self.audio_stream_with_callback.take() {
            stream.pause().expect("TODO a paused stream");
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

const AUDIO_SCRATCH_SIZE: i32 = 1024 * 8;

pub struct NesBundlerAudioCallback {
    tmp: [f32; AUDIO_SCRATCH_SIZE as usize],
    rx: Option<AudioConsumer>,
    give_back: Sender<AudioConsumer>,
    volume: Arc<AtomicU8>,
}

impl AudioCallback<f32> for NesBundlerAudioCallback {
    fn callback(&mut self, stream: &mut AudioStream2, requested: i32) {
        if let Some(rx) = &mut self.rx {
            let want = requested.min(self.tmp.len() as i32) as usize;
            let buf = &mut self.tmp[..want];
            let n = rx.pop_slice(buf);

            // Apply volume
            let gain = (self.volume.load(std::sync::atomic::Ordering::Relaxed) as f32) / 100.0;
            for x in &mut buf[..n] {
                *x *= gain;
            }

            // zero-pad if we under-ran so we still hand over 'want' frames
            if n < want {
                log::warn!("Buffer underrun ({n} < {requested})");
                buf[n..want].fill(0.0);
            }
            let _ = stream.put_data_f32(&self.tmp[..want]); // Ignore errors in callback
        }
    }
}

impl Drop for NesBundlerAudioCallback {
    fn drop(&mut self) {
        self.give_back
            .send(self.rx.take().expect("an audio consumer"))
            .expect("TODO: To be able to give back the audio consumer");
    }
}
