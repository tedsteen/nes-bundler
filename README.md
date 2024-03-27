# NES Bundler

**Transform your NES-game into a single executable targeting your favourite OS!**

Did you make a NES-game but none of your friends own a Nintendo? Don't worry.  
Add your ROM and configure NES Bundler to build for Mac, Windows and Linux.  
What you get is an executable with
* Simple UI for settings (Show and hide with ESC).
* Re-mappable Keyboard and Gamepad input (you bundle your default mappings).
* Save/Restore state (F1 = Save, F2 = Load).
* Netplay! (Optional feature, can be disabled if not wanted).

<p align="center">
  <img src="https://github.com/tedsteen/nes-bundler/blob/master/screenshot.png?raw=true" alt="Data Man!"/>
</p>

## Try it out

Before you make a proper bundle with your own icons and installer graphics you can try out NES Bundler by downloading [your binary of choice](https://github.com/tedsteen/nes-bundler/releases/).  
Running that will start a demo bundle, but if you place your own [config.yaml and/or rom.nes](config/) in the same directory as the executable it will use that.

## Proper bundling

To create a bundle you first need to [configure it](config/README.md) with your ROM and a bundle configuration and then let the GitHub Bundle action build it.  
1. Head over to the [Bundle action](https://github.com/tedsteen/nes-bundler/actions/workflows/bundle.yml)
1. Click "Run workflow"
2. For branch, select the tag of the version you want to use (must be v1.1.3 or later) or use master if you are brave
3. Paste the [URL to your bundle configuration zip](https://github.com/tedsteen/nes-bundler/blob/master/config/README.md#prepare-the-configuration-for-the-github-bundle-action) (don't worry it won't show up in the action log)
4. Run it!

If everything goes well you should receive emails with the bundles.
