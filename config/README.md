# Configure your bundle

In order to build your bundle you need two files in this directory.  

* A `build_config.yaml` containing the build configuration.
* A `nes.rom` with your game.

## Build configuration

A file named `build_config.yaml` looking something like this:
```yaml
# For all the gory details see the `BuildConfiguration`-struct in the source.

# The title of the window...
window_title: "My Awesome Game!"

# This will be the default settings for the bundle.
default_settings:
    audio:
        latency: 1 #in ms
        volume: 100 #in %
    input:
        # Two ids that corresponds to the selected input mapping configuration of P1 and P2. Should only be keyboard mappings as they're guaranteed to be available.
        selected:
            - 00-keyboard-1
            - 00-keyboard-2
        # A list of input mapping configurations. For more key mappings see https://docs.rs/winit/latest/winit/event/enum.VirtualKeyCode.html.
        # To add a gamepad configuration use the kind `Gamepad` and id `01-gamepad-0` for the first gamepad that connects, `01-gamepad-1` for the second and so on.
        configurations:
            00-keyboard-1:
                id: 00-keyboard-1
                name: "Keyboard primary"
                kind:
                    Keyboard:
                        up: Up
                        down: Down
                        left: Left
                        right: Right
                        start: Return
                        select: RShift
                        b: Key1
                        a: Key2
            00-keyboard-2:
                id: 00-keyboard-2
                name: "Keyboard secondary"
                kind:
                    Keyboard:
                        up: W
                        down: S
                        left: A
                        right: D
                        start: Key9
                        select: Key0
                        b: LAlt
                        a: LControl
        # The default mapping for newly connected gamepads. For more gamepad button mappings see https://docs.rs/gilrs/latest/gilrs/ev/enum.Button.html.
        default_gamepad_mapping:
            up: DPadUp
            down: DPadDown
            left: DPadLeft
            right: DPadRight
            start: Start
            select: Select
            b: West
            a: South

# Netplay configuration. You can remove this if the netplay feature is disabled.
netplay:
    # A universally unique identifier that identifies this particular build. Meant for builds targeting specifik users.
    # If not set, it will get assigned at runtime and saved in the settings.yaml.
    # This id will be used when querying server configurations.
    netplay_id: "<some-uuid>"
    # The default room name when starting a new netplay game
    default_room_name: ""
    # GGRS and Matchbox server configuration
    server:
        # A static configuration for Matchbox and GGRS, you can read more about them over here https://github.com/johanhelsing/matchbox and here https://github.com/gschup/ggrs
        Static:
            ggrs:
                max_prediction: 12
                input_delay: 2
            matchbox:
                # For quick and easy setup see https://github.com/tedsteen/nes-bundler/tree/master/matchbox_server.
                server: "matchbox.your-domain.io:3536"
                ice:
                    credentials:
                        # NOTE! - If you choose to put actual credentials here you should know there are risk.
                        None:
                    urls:
                        - "stun:stun.l.google.com:19302"
                        # - "turn:turn.your-domain.io:443"
```
## ROM-file

A file named `rom.nes` containing your actual game.  
You can try it out with the included `demo.nes`.
