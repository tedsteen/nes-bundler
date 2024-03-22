use std::{env, fs::File, io::Write};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tinytemplate::TinyTemplate;

#[derive(Deserialize, Serialize, Clone)]
struct BundleConfiguration {
    name: String,
    short_description: String,
    version: Option<String>,
    cf_bundle_identifier: String,
    wix_upgrade_code: String,
    manufacturer: String,
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=config/config.yaml");
    println!("cargo:rerun-if-changed=config/rom.nes");
    println!("cargo:rerun-if-changed=config/netplay-rom.nes");
    println!("cargo:rerun-if-changed=config/linux/bundle.desktop-template");
    println!("cargo:rerun-if-changed=config/macos/Info.plist-template");
    println!("cargo:rerun-if-changed=config/windows/wix/main.wxs-template");

    let mut bundle_config: BundleConfiguration =
        serde_yaml::from_str(include_str!("config/config.yaml"))?;

    if bundle_config.version.is_none() {
        bundle_config.version = Some(env!("CARGO_PKG_VERSION").to_string());
    }

    #[cfg(windows)]
    {
        fn read_semver(version: &str) -> anyhow::Result<(u64, u64, u64)> {
            let mut input = version.split('.');
            match (input.next(), input.next(), input.next()) {
                (Some(major), Some(minor), Some(patch)) => {
                    Ok((major.parse()?, minor.parse()?, patch.parse()?))
                }
                _ => Err(anyhow::Error::msg(format!(
                    "Could not parse '{version}' as semantic version"
                ))),
            }
        }

        let mut res = winres::WindowsResource::new();
        res.set_icon("config/windows/app.ico");
        res.set("FileDescription", &bundle_config.short_description);
        res.set("ProductName", &bundle_config.name);
        res.set("OriginalFilename", &format!("{}.exe", bundle_config.name));
        if let Some(version) = &bundle_config.version {
            res.set("FileVersion", version);
            res.set("ProductVersion", version);
            match read_semver(version) {
                Ok((major, minor, patch)) => {
                    let version = major << 48 | minor << 32 | patch << 16;
                    res.set_version_info(winres::VersionInfo::FILEVERSION, version);
                    res.set_version_info(winres::VersionInfo::PRODUCTVERSION, version);
                }
                Err(e) => {
                    panic!("Could not read semantic version: {:?}", e);
                }
            }
        }
        res.compile().expect("Could not attach exe icon");
    }

    let mut tt = TinyTemplate::new();

    tt.add_template(
        "main.wxs",
        include_str!("config/windows/wix/main.wxs-template"),
    )?;
    tt.add_template(
        "bundle.desktop",
        include_str!("config/linux/bundle.desktop-template"),
    )?;
    tt.add_template(
        "Info.plist",
        include_str!("config/macos/Info.plist-template"),
    )?;

    File::create("config/windows/wix/main.wxs")?
        .write_all(tt.render("main.wxs", &bundle_config)?.as_bytes())?;

    File::create("config/linux/bundle.desktop")?
        .write_all(tt.render("bundle.desktop", &bundle_config)?.as_bytes())?;

    File::create(std::path::Path::new("config/macos/Info.plist"))?
        .write_all(tt.render("Info.plist", &bundle_config)?.as_bytes())?;
    Ok(())
}
