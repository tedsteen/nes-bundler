# Configure your bundle

In order to build your bundle you need three files.  

* A `config.yaml` containing the build configuration.
* A `rom.nes` with your game.
* A `netplay-rom.nes` used when starting netplay sessions (optional if not using the netplay feature).

This directory has a pre-configured bundle that you can have a look at.

## Build configuration

A file named `config.yaml` containing your bundle configuration. Look at the demo configuration in this directory for details.  

## ROM-files

A file named `rom.nes` containing your actual game. This will be used when playing local non-netplay games.   

If you use the netplay feature you also need a file named `netplay-rom.nes`. This will be used when playing netplay games.  
It enables a different player experience for netplayers, f.ex if in a netplay session you do not want to present the player with the one player option you can bake a ROM that defaults to two players and use that as your `netplay-rom.nes`. If not, just copy the `rom.nes`.