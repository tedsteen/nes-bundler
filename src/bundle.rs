use anyhow::Result;
use serde::Deserialize;

use crate::settings::Settings;

#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
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

impl LoadBundle for Bundle {
    fn load() -> Result<Bundle> {
        Ok(Bundle {
            config: serde_yaml::from_str(include_str!("../config/config.yaml"))?,
            rom: include_bytes!("../config/rom.nes").to_vec(),
            #[cfg(feature = "netplay")]
            netplay_rom: include_bytes!("../config/netplay-rom.nes").to_vec(),
        })
    }
}
