# NES Bundler

**Transform your NES-game into a single executable targeting your favourite OS!**

Did you make a NES-game but none of your friends own a Nintendo? Don't worry.  
Put your ROM and configure NES Bundler to build for Mac, Windows and Linux.  
What you get is an executable bundle with
* Simple UI for settings (Show and hide with ESC).
* Re-mappable Keyboard and Gamepad input (you bundle your default mappings).
* Save/Restore state (F1 = Save, F2 = Load).
* Netplay! (Optional feature, can be disabled if not wanted).

<p align="center">
  <img src="https://github.com/tedsteen/nes-bundler/blob/master/screenshot.png?raw=true" alt="Super Mario!"/>
</p>

## Bundling

To create a bundle you first need to [configure it](config/README.md) with your ROM and a build configuration.  
And then let GitHub actions build the bundles for you.  
1. Fork this repository
2. [configure](config/README.md) your bundle
3. Trigger a build by running `./release.sh <your-version>`
4. Pick up the bundles in [releases](releases/)

## Demo bundle

To see an example of what you will get when creating a bundle check out [releases](https://github.com/tedsteen/nes-bundler/releases). This is the result of the demo bundle currently in [config/](config/)

## Limitations

* It's built on [rusticnes-core](https://github.com/zeta0134/rusticnes-core) so it's limited to what that can emulate (f.ex no PAL).
* Save/load state and thus netplay is currently working for mappers nrom, mmc1, mmc3, uxrom, axrom, bnrom, cnrom, gxrom and ines31.  
  If you want to contribute, please implement save/load for a mapper [over here](https://github.com/tedsteen/rusticnes-core-for-nes-bundler/blob/master/src/mmc/mapper.rs#L43-L45).