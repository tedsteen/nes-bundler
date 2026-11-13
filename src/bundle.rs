use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::Result;
use directories::ProjectDirs;
use serde::Deserialize;

use crate::{emulation::NesRegion, input::gui::InputButtonsVoca, settings::Settings};

#[derive(Deserialize, Default, Debug)]
pub struct Vocabulary {
    #[serde(default = "Default::default")]
    pub input_buttons: InputButtonsVoca,

    #[cfg(feature = "netplay")]
    #[serde(default = "Default::default")]
    pub netplay: crate::netplay::gui::NetplayVoca,
}

#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    pub name: String,
    pub manufacturer: String,
    pub default_settings: Settings,
    pub supported_nes_regions: Vec<NesRegion>,
    /// No longer used: nes-bundler always syncs to the display. Read only to warn about it.
    #[serde(default)]
    pub enable_vsync: Option<bool>,
    #[serde(default = "Default::default")]
    pub start_in_fullscreen: bool,
    #[serde(default = "Default::default")]
    pub vocabulary: Vocabulary,

    #[cfg(feature = "netplay")]
    pub netplay: crate::netplay::configuration::NetplayBuildConfiguration,
}

impl BuildConfiguration {
    pub fn default_region(&self) -> &NesRegion {
        self.supported_nes_regions
            .first()
            .expect("at least one supported nes region")
    }
    pub fn config_dir(&self) -> Option<PathBuf> {
        let path = ProjectDirs::from("", &self.manufacturer, &self.name)
            .map(|pd| pd.config_dir().to_path_buf());
        if let Some(path) = path.clone()
            && let Err(e) = fs::create_dir_all(path)
        {
            log::error!("Could not create path: {:?}", e);
        }
        path
    }
}

pub struct Bundle {
    pub settings_path: PathBuf,
    pub config: BuildConfiguration,
    pub rom: Vec<u8>,
    #[cfg(feature = "netplay")]
    pub netplay_rom: Vec<u8>,
}
impl Bundle {
    pub fn current() -> &'static Bundle {
        static MEM: OnceLock<Bundle> = OnceLock::new();
        MEM.get_or_init(|| Bundle::load().expect("bundle to load"))
    }

    fn load() -> Result<Bundle> {
        let external_config = match fs::read_to_string(Path::new("config.yaml")) {
            Ok(config) => serde_yaml_ng::from_str(&config)
                .inspect_err(|e| {
                    log::error!("config.yaml is invalid, using the bundled one instead: {e}")
                })
                .ok(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("No external config.yaml, using the bundled one");
                None
            }
            Err(e) => {
                log::error!("Could not read config.yaml, using the bundled one instead: {e}");
                None
            }
        };

        let external_rom = fs::read(Path::new("rom.nes"))
            .inspect_err(|e| log::info!("Not using external rom.nes: {:?}", e));

        // Try to load from external bundle first and if that doesn't work fall back to the embedded bundle

        let config: BuildConfiguration = match external_config {
            Some(config) => config,
            None => serde_yaml_ng::from_str(include_str!("../config/config.yaml"))?,
        };

        let rom = external_rom.unwrap_or(include_bytes!("../config/rom.nes").to_vec());

        let settings_path = config.config_dir().unwrap_or(Path::new("").to_path_buf());

        log::debug!("Settings path: {:?}", settings_path);

        Ok(Bundle {
            settings_path,
            config,
            rom,

            #[cfg(feature = "netplay")]
            netplay_rom: fs::read(Path::new("netplay-rom.nes"))
                .inspect_err(|e| log::info!("Not using external netplay-rom.nes: {:?}", e))
                .unwrap_or(include_bytes!("../config/netplay-rom.nes").to_vec()),
        })
    }
}
