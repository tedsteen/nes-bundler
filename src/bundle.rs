use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::Result;
use directories::ProjectDirs;
use serde::Deserialize;

use crate::settings::Settings;

#[derive(Deserialize, Debug)]
pub enum NesRegion {
    Pal,
    Ntsc,
    Dendy,
}

impl NesRegion {
    #[allow(dead_code)] //Needed by netplay
    pub fn to_fps(&self) -> f32 {
        match self {
            NesRegion::Pal => 50.006_977,
            NesRegion::Ntsc => 60.098_812,
            NesRegion::Dendy => 50.006_977,
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    pub name: String,
    pub manufacturer: String,
    pub default_settings: Settings,
    pub nes_region: NesRegion,
    #[cfg(feature = "netplay")]
    pub netplay: crate::netplay::NetplayBuildConfiguration,
}

impl BuildConfiguration {
    pub fn get_config_dir(&self) -> Option<PathBuf> {
        let path = ProjectDirs::from("", &self.manufacturer, &self.name)
            .map(|pd| pd.config_dir().to_path_buf());
        if let Some(path) = path.clone() {
            if let Err(e) = fs::create_dir_all(path) {
                log::error!("Could not create path: {:?}", e);
            }
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
        let external_config = fs::read_to_string(Path::new("config.yaml"))
            .map_err(anyhow::Error::msg)
            .and_then(|config| serde_yaml::from_str(&config).map_err(anyhow::Error::msg))
            .inspect_err(|e| log::info!("Not using external config.yaml: {:?}", e));

        let external_rom = fs::read(Path::new("rom.nes"))
            .inspect_err(|e| log::info!("Not using external rom.nes: {:?}", e));

        // Try to load from external bundle first and if that doesn't work fall back to the embedded bundle

        let config: BuildConfiguration =
            external_config.unwrap_or(serde_yaml::from_str(include_str!("../config/config.yaml"))?);

        let rom = external_rom.unwrap_or(include_bytes!("../config/rom.nes").to_vec());

        let settings_path = config
            .get_config_dir()
            .unwrap_or(Path::new("").to_path_buf());

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
