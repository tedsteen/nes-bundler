# Configure your bundle

In order to build your bundle you need two files in this directory.  

* A `build_config.yaml` containing the build configuration
* A `nes.rom` with your game

## Build configuration

A file named `build_config.yaml` looking something like this:
```yaml

# The title of the window...
window_title: "My Awesome Game!"
netplay:
    # If you have netplay enabled you need to provide a matchbox server.  
    # You can read all about it here https://github.com/johanhelsing/matchbox,
    # but for quick and easy setup see https://github.com/tedsteen/nes-bundler/tree/master/matchbox_server
    matchbox_server: "matchbox.your-domain.io:3536"
# This will be the default settings for the bundle.
default_settings:
    audio:
        latency: 40 #in ms
        volume: 100 #in %
    input:
        # Two ids that corresponds to the selected input mapping configuration of P1 and P2. Should only be keyboard mappings as they're guaranteed to be available.
        selected:
            - 00-keyboard-1
            - 00-keyboard-2
        # A list of input mapping configurations.For more key mappings see https://docs.rs/winit/latest/winit/event/enum.VirtualKeyCode.html
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
        # The default mapping for newly connected gamepads
        # For more gamepad button mappings see https://docs.rs/gilrs/latest/gilrs/ev/enum.Button.html
        default_gamepad_mapping:
            up: DPadUp
            down: DPadDown
            left: DPadLeft
            right: DPadRight
            start: Start
            select: Select
            b: West
            a: South
```
## ROM-file

A file named `rom.nes` containing your actual game.  
You can try it out with the included `demo.nes`.
