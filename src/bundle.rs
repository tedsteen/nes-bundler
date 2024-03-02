use std::{fs, path::Path};

use anyhow::Result;
use image::DynamicImage;
use serde::Deserialize;

use crate::settings::Settings;

#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    pub name: String,
    pub default_settings: Settings,
    #[cfg(feature = "netplay")]
    pub netplay: crate::netplay::NetplayBuildConfiguration,
}
pub trait LoadBundle {
    fn load() -> Result<Bundle>;
}

pub struct Bundle {
    pub window_icon: Option<DynamicImage>,
    pub config: BuildConfiguration,
    pub rom: Vec<u8>,
    #[cfg(feature = "netplay")]
    pub netplay_rom: Vec<u8>,
}

impl LoadBundle for Bundle {
    fn load() -> Result<Bundle> {
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
        Ok(Bundle {
            window_icon: external_windows_icon
                .unwrap_or(
                    image::load_from_memory(include_bytes!("../config/windows/icon_256x256.ico"))
                        .map_err(anyhow::Error::msg),
                )
                .ok(),

            config: external_config
                .unwrap_or(serde_yaml::from_str(include_str!("../config/config.yaml"))?),

            rom: external_rom.unwrap_or(include_bytes!("../config/rom.nes").to_vec()),

            #[cfg(feature = "netplay")]
            netplay_rom: fs::read(Path::new("netplay-rom.nes"))
                .inspect_err(|e| log::info!("Not using external netplay-rom.nes: {:?}", e))
                .unwrap_or(include_bytes!("../config/netplay-rom.nes").to_vec()),
        })
    }
}
