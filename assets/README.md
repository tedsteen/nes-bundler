# Things needed for building your bundle

In order to build your bundle you need two files in this directory.  

* A `build_config.json` containing the build configuration
* A `nes.rom` with your game

## Build configuration

A file named `build_config.json` looking something like this (a more detailed description further below):
```json
{
    "window_title": "My Awesome Game!",
    "netplay": {
        "matchbox_server": "matchbox.your-domain.io:3536"
    },
    "default_settings": {
        "audio": {
            "latency": 40,
            "volume": 100
        },
        "input": {
            "selected": [
                "00-keyboard-1",
                "00-keyboard-2"
            ],
            "configurations": {
                "00-keyboard-1": {
                    "id": "00-keyboard-1",
                    "name": "Keyboard mapping #1",
                    "disconnected": false,
                    "kind": {
                        "Keyboard": {
                            "up": "Up",
                            "down": "Down",
                            "left": "Left",
                            "right": "Right",
                            "start": "Return",
                            "select": "RShift",
                            "b": "Key1",
                            "a": "Key2"
                        }
                    }
                },
                "00-keyboard-2": {
                    "id": "00-keyboard-2",
                    "name": "Keyboard mapping #2",
                    "disconnected": false,
                    "kind": {
                        "Keyboard": {
                            "up": "W",
                            "down": "S",
                            "left": "A",
                            "right": "D",
                            "start": "Key9",
                            "select": "Key0",
                            "b": "LAlt",
                            "a": "LControl"
                        }
                    }
                }
            }
        }
    }
}
```
### `window_title`

The title of the window...

### `netplay`

If you have netplay enabled you need to provide a `matchbox_server`.  
You can read all about it [here](https://github.com/johanhelsing/matchbox), but for quick and easy setup have a look [over here](../matchbox_server/).

### `default_settings`

This will be the default settings for the bundle.
#### `audio`

Right now only audio `latency`.
#### `input`
 * `selected` - Two ids that corresponds to the selected input of P1 and P2.
 * `configurations` - A list of input mapping configurations.  It's pretty self explanatory, but if you want to read more about how to do the input mapping look [here for keyboard](https://docs.rs/winit/latest/winit/event/enum.VirtualKeyCode.html) and [here for gamepads](https://docs.rs/gilrs/latest/gilrs/ev/enum.Button.html).  

Regarding gamepads, they will get assigned ids like `01-gamepad-0`, `01-gamepad-1`, `01-gamepad-2` etc. So if you want to give a default configuration for the first gamepad that connects, add something like the following under `default_settings.input.configurations`.
```json
"01-gamepad-0": {
    "id": "01-gamepad-0",
    "name": "Gamepad #0",
    "disconnected": true,
    "kind": {
        "Gamepad": {
            "up": "DPadUp",
            "down": "DPadDown",
            "left": "DPadLeft",
            "right": "DPadRight",
            "start": "Start",
            "select": "Select",
            "b": "West",
            "a": "South"
        }
    }
}
```
All gamepads without default configurations will get the same configuration as in the example above (with different name and id).
## ROM-file

A file named `rom.nes` containing your actual game.  
You can try it out with the `demo.nes` included.
