use std::{fs, path::Path};

use anyhow::Result;
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
    pub config: BuildConfiguration,
    pub rom: Vec<u8>,
    #[cfg(feature = "netplay")]
    pub netplay_rom: Vec<u8>,
}

#[cfg(feature = "netplay")]
const NETPLAY_ROM: &[u8] = include_bytes!("../config/netplay-rom.nes");

fn load_external_bundle() -> Result<Option<Bundle>> {
    let config_path = Path::new("config.yaml");
    let rom_path = Path::new("rom.nes");
    if config_path.exists() && rom_path.exists() {
        let config = fs::read_to_string(config_path)?;
        let config = serde_yaml::from_str(&config)?;
        let rom = fs::read(rom_path)?;

        let netplay_rom = fs::read(Path::new("netplay-rom.nes")).unwrap_or_else(|e| {
            log::warn!(
                "Could not load custom netplay rom ({:?}), falling back on default",
                e
            );
            rom.clone()
        });

        return Ok(Some(Bundle {
            config,
            rom,
            #[cfg(feature = "netplay")]
            netplay_rom,
        }));
    }
    Ok(None)
}

impl LoadBundle for Bundle {
    fn load() -> Result<Bundle> {
        let external_bundle = load_external_bundle();
        match external_bundle {
            Ok(Some(bundle)) => return Ok(bundle),
            Err(e) => log::warn!("Failed to load external bundle: {:}", e),
            _ => {}
        }
        Ok(Bundle {
            config: serde_yaml::from_str(include_str!("../config/config.yaml"))?,
            rom: include_bytes!("../config/rom.nes").to_vec(),
            #[cfg(feature = "netplay")]
            netplay_rom: NETPLAY_ROM.to_vec(),
        })
    }
}
