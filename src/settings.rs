use crate::{
    audio::AudioSettings,
    bundle::Bundle,
    emulation::NesRegion,
    input::{InputConfigurationKind, settings::InputSettings},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    fs::{self, File},
    hash::{Hash, Hasher},
    io::{BufReader, BufWriter, ErrorKind, Write},
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Condvar, Mutex, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Duration,
};

pub const MAX_PLAYERS: usize = 2;

pub struct AutoSavingSettings<'a> {
    inner: RwLockWriteGuard<'a, Settings>,
    hash_before: u64,
}

impl<'a> AutoSavingSettings<'a> {
    fn new(inner: &'a RwLock<Settings>) -> Self {
        let inner = inner.write().unwrap();
        AutoSavingSettings {
            hash_before: inner.get_hash(),
            inner,
        }
    }
}

impl Deref for AutoSavingSettings<'_> {
    type Target = Settings;

    fn deref(&self) -> &Settings {
        &self.inner
    }
}

impl DerefMut for AutoSavingSettings<'_> {
    fn deref_mut(&mut self) -> &mut Settings {
        &mut self.inner
    }
}

impl Drop for AutoSavingSettings<'_> {
    fn drop(&mut self) {
        if self.hash_before != self.inner.get_hash() {
            SettingsWriter::global().queue(self.inner.clone());
        }
    }
}

/// Writes settings to disk off the threads that change them: a change is queued, and written
/// once changes have stopped for a moment, so dragging a slider writes once rather than every
/// frame, and nobody waits on the disk while holding the settings lock.
struct SettingsWriter {
    pending: Mutex<Option<Settings>>,
    changed: Condvar,
}

impl SettingsWriter {
    /// How long settings have to stay unchanged before they're written.
    const QUIET: Duration = Duration::from_millis(500);

    fn global() -> &'static Self {
        static WRITER: OnceLock<SettingsWriter> = OnceLock::new();
        WRITER.get_or_init(|| {
            std::thread::Builder::new()
                .name("settings-writer".to_string())
                .spawn(|| Self::global().run())
                .expect("a thread to write settings");
            Self {
                pending: Mutex::new(None),
                changed: Condvar::new(),
            }
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Settings>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn queue(&self, settings: Settings) {
        *self.lock() = Some(settings);
        self.changed.notify_one();
    }

    fn run(&self) {
        loop {
            let mut pending = self.lock();
            while pending.is_none() {
                pending = self
                    .changed
                    .wait(pending)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            // Wait for the changes to stop: each new one restarts the wait.
            loop {
                let (guard, wait) = self
                    .changed
                    .wait_timeout(pending, Self::QUIET)
                    .unwrap_or_else(PoisonError::into_inner);
                pending = guard;
                if wait.timed_out() {
                    break;
                }
            }
            let settings = pending.take();
            drop(pending);
            if let Some(settings) = settings {
                settings.save();
            }
        }
    }

    /// Writes what's queued now, for when the app is about to quit.
    fn flush(&self) {
        let settings = self.lock().take();
        if let Some(settings) = settings {
            settings.save();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Hash)]
pub struct Settings {
    pub audio: AudioSettings,
    pub input: InputSettings,
    pub netplay_id: Option<String>,
    pub save_state: Option<String>,
    nes_region: Option<NesRegion>,
}

pub struct SettingsStore {
    inner: RwLock<Settings>,
}

impl SettingsStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Settings::load()),
        }
    }

    pub fn global() -> &'static Self {
        static MEM: OnceLock<SettingsStore> = OnceLock::new();
        MEM.get_or_init(SettingsStore::new)
    }

    pub fn write(&self) -> AutoSavingSettings<'_> {
        AutoSavingSettings::new(&self.inner)
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Settings> {
        self.inner.read().unwrap()
    }

    /// Writes any settings changes not saved yet. Call before quitting.
    pub fn flush() {
        SettingsWriter::global().flush();
    }
}

impl Settings {
    pub fn current_mut<'a>() -> AutoSavingSettings<'a> {
        SettingsStore::global().write()
    }

    pub fn current<'a>() -> RwLockReadGuard<'a, Settings> {
        SettingsStore::global().read()
    }

    #[cfg(feature = "netplay")]
    pub fn ensure_netplay_id(&mut self) -> String {
        self.netplay_id
            .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .to_string()
    }

    fn path() -> PathBuf {
        Bundle::current().settings_path.join("settings.yaml")
    }

    fn load() -> Settings {
        let bundle = Bundle::current();
        let settings_file_path = &Self::path();
        let default_settings = bundle.config.default_settings.clone();

        let file = match File::open(settings_file_path) {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                log::info!("No settings yet, starting from the defaults");
                return default_settings;
            }
            Err(e) => {
                log::warn!("Could not open {settings_file_path:?} ({e}), using the defaults");
                return default_settings;
            }
        };
        let mut settings: Result<Settings> =
            serde_yaml_ng::from_reader(BufReader::new(file)).map_err(anyhow::Error::msg);

        match &mut settings {
            Ok(settings) => {
                // Input configurations the bundle has added since, and selections of ones that
                // are gone (renamed in a newer bundle, say), which nothing could look up.
                for (id, configuration) in &default_settings.input.configurations {
                    settings
                        .input
                        .configurations
                        .entry(id.clone())
                        .or_insert_with(|| configuration.clone());
                }
                for (selected, default) in settings
                    .input
                    .selected
                    .iter_mut()
                    .zip(&default_settings.input.selected)
                {
                    if !settings.input.configurations.contains_key(selected) {
                        log::warn!("Input {selected:?} is gone, selecting {default:?} instead");
                        selected.clone_from(default);
                    }
                }

                let default_selected = default_settings.input.selected.clone();
                //Make sure no gamepads are selected after loading settings (they will be autoselected later if they are connected)
                for (player, default_selected_input) in
                    default_selected.iter().enumerate().take(MAX_PLAYERS)
                {
                    if let InputConfigurationKind::Gamepad(_) =
                        &settings.input.selected_configuration(player).kind
                    {
                        settings.input.selected[player].clone_from(default_selected_input);
                    }
                }
            }
            Err(e) => {
                // Kept aside rather than overwritten by the defaults on the next save, since it
                // holds the battery save too.
                let backup = settings_file_path.with_extension("yaml.unreadable");
                log::warn!(
                    "Could not read {settings_file_path:?} ({e}), using the defaults; moved it to {backup:?}"
                );
                if let Err(e) = fs::rename(settings_file_path, &backup) {
                    log::error!("Could not move the unreadable settings aside: {e}");
                }
            }
        }
        settings.unwrap_or(default_settings)
    }

    /// Writes to a temporary file and renames it over the settings, so a crash mid-write leaves
    /// the old settings rather than half of the new ones.
    fn save(&self) {
        let path = Self::path();
        let temporary = path.with_extension("yaml.tmp");
        let written = File::create(&temporary)
            .map_err(anyhow::Error::msg)
            .and_then(|file| {
                let mut writer = BufWriter::new(file);
                serde_yaml_ng::to_writer(&mut writer, self).map_err(anyhow::Error::msg)?;
                writer.flush()?;
                writer.get_ref().sync_all()?;
                Ok(())
            })
            .and_then(|()| fs::rename(&temporary, &path).map_err(anyhow::Error::msg));
        match written {
            Ok(()) => log::debug!("Settings saved"),
            Err(e) => log::error!("Failed to save settings: {e:?}"),
        }
    }

    fn get_hash(&self) -> u64 {
        let hasher = &mut DefaultHasher::new();
        self.hash(hasher);
        hasher.finish()
    }

    pub fn nes_region(&self) -> NesRegion {
        self.nes_region
            .clone()
            .unwrap_or_else(|| Bundle::current().config.default_region().clone())
    }

    pub fn nes_region_mut(&mut self) -> &mut NesRegion {
        self.nes_region
            .get_or_insert_with(|| Bundle::current().config.default_region().clone())
    }
}
