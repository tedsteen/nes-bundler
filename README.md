# NES Bundler

**Transform your NES-game into a single executable targeting your favourite OS!**

Did you make a NES-game but none of your friends own a Nintendo? Don't worry.  
Add your ROM and configure NES Bundler to build for Mac, Windows and Linux.  
What you get is a digitally signed executable with
* Simple UI for settings (Show and hide with ESC).
* Re-mappable Keyboard and Gamepad input (you bundle your default mappings).
* Automatic save/load of sram state
* Netplay! (Optional feature, can be disabled if not wanted).

<p align="center">
  <img src="https://github.com/tedsteen/nes-bundler/blob/master/screenshot.gif?raw=true" alt="Data Man!"/>
</p>

## Try it out

Before you make a proper bundle with your own icons and installer graphics you can try out NES Bundler by downloading [your binary of choice](https://github.com/tedsteen/nes-bundler/releases/).  
Running that will start a demo bundle, but if you place your own [config.yaml and/or rom.nes](config/) in the same directory as the executable it will use that.

## Proper bundling

To create a bundle you need to [configure it](config/README.md) with your ROM and a bundle configuration, zip it then send it of for bundling at https://nes-bundler.com/

If everything goes well you should receive emails with the bundles.

## Building

```bash
cargo build --release
# or for dev
cargo run --profile dev
```
### Dependencies

* cmake (`brew install cmake`)
* cargo-release (`cargo install cargo-release`, only needed when releasing a new version of nes-bundler)