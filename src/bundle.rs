use anyhow::Result;
use serde::Deserialize;

use crate::settings::Settings;

#[derive(Deserialize, Debug)]
pub struct BuildConfiguration {
    pub window_title: String,
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
}
#[cfg(feature = "zip-bundle")]
impl LoadBundle for Bundle {
    fn load() -> Result<Bundle> {
        use anyhow::Context;
        if let Ok(zip_file) = std::fs::File::open("bundle.zip") {
            let mut zip = zip::ZipArchive::new(zip_file)?;
            let config: BuildConfiguration = serde_yaml::from_reader(
                zip.by_name("config.yaml")
                    .context("config.yaml not found in bundle.zip")?,
            )?;

            let mut rom = Vec::new();
            std::io::copy(
                &mut zip
                    .by_name("rom.nes")
                    .context("rom.nes not found in bundle.zip")?,
                &mut rom,
            )?;
            Ok(Bundle { config, rom })
        } else {
            let folder = rfd::FileDialog::new()
                .set_title("Files to bundle")
                .set_directory(".")
                .pick_folder()
                .context("No bundle to load")?;

            let mut config_path = folder.clone();
            config_path.push("config.yaml");
            let mut config_file = std::fs::File::open(config_path)
                .context(format!("config.yaml not found in {:?}", folder))?;

            let mut rom_path = folder.clone();
            rom_path.push("rom.nes");
            let mut rom_file = std::fs::File::open(rom_path)
                .context(format!("rom.nes not found in {:?}", folder))?;

            let mut zip = zip::ZipWriter::new(
                std::fs::File::create("bundle.zip").context("Could not create bundle.zip")?,
            );
            zip.start_file("config.yaml", Default::default())?;
            std::io::copy(&mut config_file, &mut zip)?;

            zip.start_file("rom.nes", Default::default())?;
            std::io::copy(&mut rom_file, &mut zip)?;

            zip.finish()?;

            // Try again with newly created bundle.zip
            Self::load()
        }
    }
}

#[cfg(not(feature = "zip-bundle"))]
impl LoadBundle for Bundle {
    fn load() -> Result<Bundle> {
        Ok(Bundle {
            config: serde_yaml::from_str(include_str!("../../config/config.yaml"))?,
            rom: include_bytes!("../../config/rom.nes").to_vec(),
        })
    }
}
