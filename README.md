# Nes Bundler

**Transform your NES-game to a single executable targeting your favourite OS!**

Did you make a NES-game but non of your friends owns a Nintendo? Don't worry.  
Put your ROM and configuration in this Nes Bundler and build an executable for Mac, Windows or Linux.  
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
* [Install Rust](https://www.rust-lang.org/tools/install)
* [Configure a bundle](assets/README.md) with your ROM and a build configuration.
* Build (`cargo build --release`) your exectutable!

If you want to target other operating systems please read the rust documentation or find a machine with that OS and follow the steps again.

## Limitations

It's built on [rusticnes-core](https://github.com/zeta0134/rusticnes-core) so it's limited to what that can emulate (f.ex no PAL)

## Future stuff/ideas/todo
* Input
  * Autoselect gamepads if they connect (Enable in build configuration?)
* Audio
  * More control over output device?
  * Volume slider?
* Netplay
  * Support STUN servers
  * Save and restore session - save game state every 100th or so frame (when all peers have reached that 100th frame)
  * More control on who becomes P1 and P2 etc
  * Make it possible to wait for peers to reconnect if disconnected
  * Move strange settings (max prediction + input delay) from UI -> build configuration and make UI simpler or try to auto tune it.
  * Default room name as build config
* More customizable UI
* wasm?
* ...