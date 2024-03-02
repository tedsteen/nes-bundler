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

#[cfg(feature = "netplay")]
const NETPLAY_ROM: &[u8] = include_bytes!("../config/netplay-rom.nes");
const WINDOW_ICON: &[u8] = include_bytes!("../config/windows/icon_256x256.ico");
fn load_external_bundle(default_window_icon: Option<DynamicImage>) -> Result<Option<Bundle>> {
    let config_path = Path::new("config.yaml");
    let rom_path = Path::new("rom.nes");
    if config_path.exists() && rom_path.exists() {
        let config = fs::read_to_string(config_path)?;
        let config = serde_yaml::from_str(&config)?;
        let rom = fs::read(rom_path)?;

        let window_icon = fs::read(Path::new("config/windows/icon_256x256.ico")).map_or_else(
            |_| default_window_icon,
            |image_data| image::load_from_memory(&image_data).ok(),
        );

        return Ok(Some(Bundle {
            window_icon,
            config,
            rom: rom.clone(),
            #[cfg(feature = "netplay")]
            netplay_rom: fs::read(Path::new("netplay-rom.nes")).unwrap_or_else(|e| {
                log::warn!(
                    "Could not load custom netplay rom ({:?}), falling back on default",
                    e
                );
                rom
            }),
        }));
    }
    Ok(None)
}

impl LoadBundle for Bundle {
    fn load() -> Result<Bundle> {
        let window_icon = image::load_from_memory(WINDOW_ICON).ok();
        let external_bundle = load_external_bundle(window_icon.clone());
        match external_bundle {
            Ok(Some(bundle)) => return Ok(bundle),
            Err(e) => log::warn!("Failed to load external bundle: {:}", e),
            _ => {}
        }
        Ok(Bundle {
            window_icon,
            config: serde_yaml::from_str(include_str!("../config/config.yaml"))?,
            rom: include_bytes!("../config/rom.nes").to_vec(),
            #[cfg(feature = "netplay")]
            netplay_rom: NETPLAY_ROM.to_vec(),
        })
    }
}
