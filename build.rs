use std::{fs::File, io::Write};

use anyhow::Result;
use cargo_metadata::MetadataCommand;
use serde::Serialize;
use tinytemplate::TinyTemplate;

#[derive(Serialize)]
struct Context {
    version: String,
    cf_bundle_identifier: String,
    wix_upgrade_code: String,
    bundle_name: String,
    short_description: String,
    keywords: Vec<String>,
    homepage: String,
    manufacturer: String,
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=config/config.yaml");
    println!("cargo:rerun-if-changed=config/rom.nes");
    println!("cargo:rerun-if-changed=config/netplay-rom.nes");

    println!("cargo:rerun-if-changed=os_bundle/linux/*");
    println!("cargo:rerun-if-changed=os_bundle/macos/*");
    println!("cargo:rerun-if-changed=os_bundle/windows/*");
    println!("cargo:rerun-if-changed=os_bundle/windows/wix/*");

    println!("cargo:rerun-if-changed=src/audio/stretch/signalsmith-stretch/signalsmith-stretch.h");
    println!("cargo:rerun-if-changed=src/audio/stretch/signalsmith-stretch-wrapper.hpp");
    println!("cargo:rerun-if-changed=src/audio/stretch/signalsmith-stretch-wrapper.cpp");
    println!("cargo:rerun-if-changed=src/audio/stretch/mod.rs");
    let mut code = cxx_build::bridge("src/audio/stretch/mod.rs");
    let code = code
        .file("src/audio/stretch/signalsmith-stretch-wrapper.cpp")
        .flag_if_supported("-std=c++11");

    #[cfg(not(target_os = "windows"))]
    code.flag("-O3");
    #[cfg(target_os = "windows")]
    code.flag("/O2");

    code.compile("signalsmith-stretch");

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("os_bundle/windows/icon_256x256.ico");
        res.compile().expect("Could not attach exe icon");
    }
    // println!(
    //     "cargo:warning=HELLOOO{:?}",
    //     std::env::var("OUT_DIR").unwrap()
    // );

    let path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let meta = MetadataCommand::new()
        .manifest_path("./Cargo.toml")
        .current_dir(&path)
        .exec()
        .unwrap();

    let root = meta.root_package().unwrap();
    if let Some(bundle) = root.metadata["bundle"].as_object() {
        let mut tt = TinyTemplate::new();

        tt.add_template(
            "main.wxs",
            include_str!("os_bundle/windows/wix/main.wxs-template"),
        )?;
        tt.add_template(
            "bundle.desktop",
            include_str!("os_bundle/linux/bundle.desktop-template"),
        )?;
        tt.add_template(
            "Info.plist",
            include_str!("os_bundle/macos/Info.plist-template"),
        )?;

        let keywords = bundle["keywords"]
            .as_array()
            .unwrap()
            .to_vec()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        let context = &Context {
            cf_bundle_identifier: bundle["cf_bundle_identifier"].as_str().unwrap().to_string(),
            wix_upgrade_code: bundle["wix_upgrade_code"].as_str().unwrap().to_string(),
            version: bundle["version"].as_str().unwrap().to_string(),
            bundle_name: bundle["name"].as_str().unwrap().to_string(),
            keywords,
            short_description: bundle["short_description"].as_str().unwrap().to_string(),
            homepage: bundle["homepage"].as_str().unwrap().to_string(),
            manufacturer: bundle["manufacturer"].as_str().unwrap().to_string(),
        };
        println!("cargo:rustc-env=NB_WINDOW_TITLE={}", context.bundle_name);

        File::create("os_bundle/windows/wix/main.wxs")?
            .write_all(tt.render("main.wxs", context)?.as_bytes())?;

        File::create("os_bundle/linux/bundle.desktop")?
            .write_all(tt.render("bundle.desktop", context)?.as_bytes())?;

        let path = std::path::Path::new("target/bundle/osx/nes-bundler.app/Contents/");
        std::fs::create_dir_all(path).unwrap();
        File::create(path.join("Info.plist"))?
            .write_all(tt.render("Info.plist", context)?.as_bytes())?;
    } else {
        println!("cargo:warning=No bundle :(");
    }
    Ok(())
}
