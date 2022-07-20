## Assets needed for building
In order to build you need two files in this directory.
A build configuration-file and a ROM-file.

### Build configuration
A file named `build_configuration.json` looking something like this:
```json
{
    "window_title": "My Awesome Game!",
    "default_settings": {
        "audio": {
            "latency": 40
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

### ROM-file
A file named `rom.nes` containing your actual game.
