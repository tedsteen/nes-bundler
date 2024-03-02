use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use directories::ProjectDirs;
use image::DynamicImage;
use serde::Deserialize;

use crate::settings::Settings;

#[derive(Deserialize, Debug, Clone)]
pub struct BuildConfiguration {
    pub name: String,
    pub manufacturer: String,
    pub default_settings: Settings,
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
    pub window_icon: Option<DynamicImage>,
    pub config: BuildConfiguration,
    pub rom: Vec<u8>,
    #[cfg(feature = "netplay")]
    pub netplay_rom: Vec<u8>,
}
impl Bundle {
    pub fn load() -> Result<Bundle> {
        let external_windows_icon = fs::read(Path::new("windows/icon_256x256.ico"))
            .map(|image_data| image::load_from_memory(&image_data).map_err(anyhow::Error::msg))
            .inspect_err(|e| log::info!("Not using external windows/icon_256x256.ico: {:?}", e));

        let external_config = fs::read_to_string(Path::new("config.yaml"))
            .map_err(anyhow::Error::msg)
            .and_then(|config| serde_yaml::from_str(&config).map_err(anyhow::Error::msg))
            .inspect_err(|e| log::info!("Not using external config.yaml: {:?}", e));

        let external_rom = fs::read(Path::new("rom.nes"))
            .inspect_err(|e| log::info!("Not using external rom.nes: {:?}", e));

        // Try to load from external bundle first and if that doesn't work fall back to the embedded bundle

        let config: BuildConfiguration =
            external_config.unwrap_or(serde_yaml::from_str(include_str!("../config/config.yaml"))?);

        let window_icon = external_windows_icon
            .unwrap_or(
                image::load_from_memory(include_bytes!("../config/windows/icon_256x256.ico"))
                    .map_err(anyhow::Error::msg),
            )
            .ok();

        let rom = external_rom.unwrap_or(include_bytes!("../config/rom.nes").to_vec());

        let settings_path = config
            .get_config_dir()
            .unwrap_or(Path::new("").to_path_buf())
            .join("settings.yaml");

        log::debug!("Settings path: {:?}", settings_path);

        Ok(Bundle {
            settings_path,
            window_icon,
            config,
            rom,

            #[cfg(feature = "netplay")]
            netplay_rom: fs::read(Path::new("netplay-rom.nes"))
                .inspect_err(|e| log::info!("Not using external netplay-rom.nes: {:?}", e))
                .unwrap_or(include_bytes!("../config/netplay-rom.nes").to_vec()),
        })
    }
}
