# NES Bundler

**Transform your NES-game into a single executable targeting your favourite OS!**

Did you make a NES-game but none of your friends own a Nintendo? Don't worry.  
Put your ROM and configuration in NES Bundler and build for Mac, Windows or Linux.  
What you get is a single executable with
* Simple UI for settings (Show and hide with ESC).
* Re-mappable Keyboard and Gamepad input (you bundle your default mappings).
* Save/Restore state (F1 = Save, F2 = Load).
* Netplay! (Optional feature, can be disabled if not wanted).

<p align="center">
  <img src="https://github.com/tedsteen/nes-bundler/blob/master/screenshot.png?raw=true" alt="Super Mario!"/>
</p>

## Bundling

To create a bundle you need to do the following
* [Configure a bundle](config/README.md) with your ROM and a build configuration.
* Download the [binary of your choice](https://github.com/tedsteen/nes-bundler/releases/)
* Make the bundle - `./bundle.sh <config-dir> <downloaded-binary> <name-of-output-binary>` (`config-dir` containing your configuration)

## Limitations

* It's built on [rusticnes-core](https://github.com/zeta0134/rusticnes-core) so it's limited to what that can emulate (f.ex no PAL).
* Save/load state and thus netplay is currently working for mappers nrom, mmc1, mmc3, uxrom, axrom, bnrom, cnrom, gxrom and ines31.  
  If you want to contribute, please implement save/load for a mapper [over here](https://github.com/tedsteen/rusticnes-core-for-nes-bundler/blob/master/src/mmc/mapper.rs#L43-L45).

## Future stuff/ideas/bugs/todo
* Move this list to the issues feature in GitHub :)
* Implement `save_state(...)`/`load_state(...)` for all the mappers.
* Fullscreen mode (alt+enter is standard to toggle between full screen and windowed, on Windows)
  * Some way to quit without closing the window will be needed for fullscreen. Probably a button in the settings menu.
* Perhaps freeze the game while settings is open?
* BUG (windows): You have to press a key on the gamepad before it appears as an option in settings.
* A little toast at the start that says "Press ESC to change settings"
* Netplay
  * Save and restore session - save game state every 100th or so frame (when all peers have reached that 100th frame).
  * More control on who becomes P1 and P2 etc.
  * Make it possible to wait for peers to reconnect if disconnected.
* More customizable UI.
* wasm?
* ...
