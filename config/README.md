# Configure your bundle

In order to build your bundle you need two files.  

* A `config.yaml` containing the build configuration.
* A `nes.rom` with your game.

## Build configuration

A file named `config.yaml` looking something like this:
```yaml
# For all the gory details see the `BuildConfiguration`-struct in the source.

# The title of the window...
window_title: "My Awesome Game!"

# This will be the default settings for the bundle.
default_settings:
    audio:
        latency: 10 #in ms
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
                name: "⌨ Keyboard 1"
                kind: !Keyboard
                    up: ArrowUp
                    down: ArrowDown
                    left: ArrowLeft
                    right: ArrowRight
                    start: Enter
                    select: ShiftRight
                    b: Digit1
                    a: Digit2
            00-keyboard-2:
                id: 00-keyboard-2
                name: "⌨ Keyboard 2"
                kind: !Keyboard
                    up: KeyW
                    down: KeyS
                    left: KeyA
                    right: KeyD
                    start: Digit9
                    select: Digit0
                    b: AltLeft
                    a: ControlLeft
            01-gamepad-0:
                id: 01-gamepad-0
                name: "🎮 Gamepad"
                kind: !Gamepad
                    up: DPadUp
                    down: DPadDown
                    left: DPadLeft
                    right: DPadRight
                    start: Start
                    select: Back
                    b: X
                    a: A
        # The default mapping for newly connected gamepads. For more gamepad button mappings see https://docs.rs/sdl2/latest/sdl2/controller/enum.Button.html.
        default_gamepad_mapping:
            up: DPadUp
            down: DPadDown
            left: DPadLeft
            right: DPadRight
            start: Start
            select: Back
            b: X
            a: A

# Netplay configuration. You can remove this if the netplay feature is disabled.
netplay:
    # The default room name when starting a new netplay game
    default_room_name: ""
    # GGRS and Matchbox server configuration. You can read more about them over here https://github.com/johanhelsing/matchbox and here https://github.com/gschup/ggrs
    # This config can be either fetched from an external service (TurnOn) or configured statically (Static)
    server:
        # A hosted TurnOn server which should do the job.
        # More information on this will come. It's free at the moment, but if network traffic costs starts piling up, there might be a paywall when unlocking Netplay (only needed if a direct p2p connection is not possible).
        !TurnOn "https://netplay.tech/get-config"
        # An example of a static configuration
        #!Static
        #    ggrs:
        #        max_prediction: 12
        #        input_delay: 2
        #    matchbox:
        #        # For quick and easy setup see https://github.com/tedsteen/nes-bundler/tree/master/matchbox_server.
        #        server: "matchbox.your-domain.io:3536"
        #        ice:
        #            credentials:
        #                # NOTE! - If you choose to put actual credentials here you should know there are risk.
        #                !None
        #            urls:
        #                - "stun:stun.l.google.com:19302"
    # An optional, universally unique identifier that identifies this particular build. Meant for builds targeting specific users.
    # If not set, it will get assigned at runtime and saved in the settings.yaml.
    # This id will be used when querying server configurations (TurnOn).
    #netplay_id: "<some-uuid>"
```
## ROM-file

A file named `rom.nes` containing your actual game.  
You can try it out with the included `demo.nes`.
