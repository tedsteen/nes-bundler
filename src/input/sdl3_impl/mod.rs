use sdl3::GamepadSubsystem;
use sdl3::gamepad::Gamepad;
use sdl3::joystick::JoystickId;

use super::buttons::ToGamepadButton;
use super::settings::InputSettings;
use super::{InputConfiguration, ToInputId};
use super::{InputId, JoypadState, buttons::GamepadButton};
use crate::bundle::Bundle;
use crate::input::InputConfigurationKind;
use std::collections::{HashMap, HashSet};

use super::gamepad::{GamepadEvent, GamepadState, Gamepads, JoypadGamepadMapping, ToGamepadEvent};

pub struct SDL3GamepadState {
    pub pressed_buttons: HashSet<GamepadButton>,
    game_controller: Gamepad,
}

impl SDL3GamepadState {
    pub fn new(game_controller: Gamepad) -> Self {
        Self {
            pressed_buttons: HashSet::new(),
            game_controller,
        }
    }
}

impl ToInputId for u32 {
    fn to_input_id(&self) -> InputId {
        self.to_string()
    }
}

impl ToInputId for JoystickId {
    fn to_input_id(&self) -> InputId {
        u32::from(*self).to_input_id()
    }
}

impl GamepadState for SDL3GamepadState {
    fn is_connected(&self) -> bool {
        self.game_controller.connected()
    }

    fn get_pressed_buttons(&self) -> &HashSet<GamepadButton> {
        &self.pressed_buttons
    }

    fn toggle_button(&mut self, button: &GamepadButton, pressed: bool) {
        if pressed {
            self.pressed_buttons.insert(*button);
        } else {
            self.pressed_buttons.remove(button);
        }
    }
}
pub struct SDL3Gamepads {
    game_controller_subsystem: GamepadSubsystem,
    /// By input configuration id, one per slot.
    all: HashMap<InputId, Box<dyn GamepadState>>,
    /// SDL's id for each connected gamepad, to its input configuration id.
    by_instance: HashMap<InputId, InputId>,
}

impl Gamepads for SDL3Gamepads {
    fn get_joypad(&self, id: &InputId, mapping: &JoypadGamepadMapping) -> JoypadState {
        if let Some(state) = self.get_gamepad_by_input_id(id) {
            mapping.calculate_state(state.get_pressed_buttons())
        } else {
            JoypadState(0)
        }
    }

    fn get_gamepad_by_input_id(&self, id: &InputId) -> Option<&dyn GamepadState> {
        self.all.get(id).map(|a| a.as_ref())
    }

    fn advance(&mut self, gamepad_event: &GamepadEvent, input_settings: &mut InputSettings) {
        match gamepad_event {
            GamepadEvent::ControllerAdded { which, .. } => {
                if !self.setup_gamepad_config(which.clone(), input_settings) {
                    log::error!("Could not setup controller {:?}", which);
                }
            }
            // Its slot frees up, and `Inputs` moves a player using it back to a keyboard.
            GamepadEvent::ControllerRemoved { which } => {
                if let Some(gamepad_id) = self.by_instance.remove(which) {
                    self.all.remove(&gamepad_id);
                    log::info!("Gamepad {gamepad_id} disconnected");
                }
            }
            GamepadEvent::ButtonDown { which, button, .. } => {
                if let Some(gamepad_state) = self.get_gamepad(which.clone()) {
                    gamepad_state.toggle_button(button, true);
                } else {
                    log::warn!("Button down on unmapped gamepad {:?}", which);
                }
            }
            GamepadEvent::ButtonUp { which, button, .. } => {
                if let Some(gamepad_state) = self.get_gamepad(which.clone()) {
                    gamepad_state.toggle_button(button, false);
                } else {
                    log::warn!("Button up on unmapped gamepad {:?}", which);
                }
            }
        }
    }
}
impl SDL3Gamepads {
    /// Gamepads are known by slot: the first connected is 0, the next 1, and one plugged back in
    /// takes the lowest free slot. SDL's own ids count up with every connection, so keying by
    /// them would never match the bundle's preset (`01-gamepad-0`) and would lose remaps on
    /// every replug.
    fn to_gamepad_id(slot: usize) -> String {
        format!("01-gamepad-{slot}")
    }

    pub fn new(game_controller_subsystem: GamepadSubsystem) -> Self {
        SDL3Gamepads {
            game_controller_subsystem,
            all: HashMap::new(),
            by_instance: HashMap::new(),
        }
    }

    fn get_gamepad(&mut self, instance_id: InputId) -> Option<&mut Box<dyn GamepadState>> {
        let gamepad_id = self.by_instance.get(&instance_id)?;
        self.all.get_mut(gamepad_id)
    }

    /// The lowest slot no connected gamepad has.
    fn free_slot(&self) -> usize {
        (0..)
            .find(|&slot| {
                self.all
                    .get(&Self::to_gamepad_id(slot))
                    .is_none_or(|gamepad| !gamepad.is_connected())
            })
            .expect("a free slot")
    }

    /// Opens a newly connected controller, registers it, creates or retrieves its input
    /// configuration, and auto-selects it for any player currently using a keyboard.
    ///
    /// All settings mutations are done under a single lock acquisition to avoid
    /// saving the settings file twice and to prevent a window between two separate writes.
    ///
    /// Returns `true` if the controller was successfully set up, `false` otherwise.
    fn setup_gamepad_config(
        &mut self,
        input_id: InputId,
        input_settings: &mut InputSettings,
    ) -> bool {
        let gamepads = match self.game_controller_subsystem.gamepads() {
            Ok(gamepads) => gamepads,
            Err(e) => {
                log::error!("Could not list gamepads: {e}");
                return false;
            }
        };
        let Some(found_controller) = gamepads.iter().find_map(|id| {
            if input_id == id.to_input_id() && self.game_controller_subsystem.is_gamepad(*id) {
                match self.game_controller_subsystem.open(*id) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        log::error!("Failed to open controller {:?}", e);
                        None
                    }
                }
            } else {
                None
            }
        }) else {
            return false;
        };

        let slot = self.free_slot();
        let gamepad_id = Self::to_gamepad_id(slot);
        self.by_instance.retain(|_, id| *id != gamepad_id);
        self.by_instance.insert(input_id, gamepad_id.clone());
        self.all.insert(
            gamepad_id.clone(),
            Box::new(SDL3GamepadState::new(found_controller)),
        );
        log::info!("Gamepad connected as {gamepad_id}");

        // The caller already holds the settings write lock; taking it again here deadlocks.
        let default_mapping = input_settings.default_gamepad_mapping;
        // Named for its slot, unless the bundle names it; set on every connection, since older
        // versions named gamepads after SDL's ids.
        let name = Bundle::current()
            .config
            .default_settings
            .input
            .configurations
            .get(&gamepad_id)
            .map_or_else(|| format!("🎮 Gamepad {}", slot + 1), |c| c.name.clone());
        let configuration = input_settings.get_or_create_config(
            gamepad_id.clone(),
            InputConfiguration {
                name: name.clone(),
                id: gamepad_id,
                kind: InputConfigurationKind::Gamepad(default_mapping),
            },
        );
        configuration.name = name;
        let conf_id = configuration.id.clone();

        // Automatically select this gamepad for the first player still using a keyboard.
        if matches!(
            input_settings.selected_configuration(0).kind,
            InputConfigurationKind::Keyboard(_)
        ) {
            input_settings.selected[0] = conf_id;
        } else if matches!(
            input_settings.selected_configuration(1).kind,
            InputConfigurationKind::Keyboard(_)
        ) {
            input_settings.selected[1] = conf_id;
        }

        true
    }
}

impl ToGamepadEvent for sdl3::event::Event {
    fn to_gamepad_event(&self) -> Option<GamepadEvent> {
        match self {
            sdl3::event::Event::GamepadAdded { which, .. } => Some(GamepadEvent::ControllerAdded {
                which: which.to_input_id(),
            }),
            sdl3::event::Event::GamepadRemoved { which, .. } => {
                Some(GamepadEvent::ControllerRemoved {
                    which: which.to_input_id(),
                })
            }
            sdl3::event::Event::GamepadButtonDown { which, button, .. } => button
                .to_gamepad_button()
                .map(|button| GamepadEvent::ButtonDown {
                    which: which.to_input_id(),
                    button,
                }),
            sdl3::event::Event::GamepadButtonUp { which, button, .. } => button
                .to_gamepad_button()
                .map(|button| GamepadEvent::ButtonUp {
                    which: which.to_input_id(),
                    button,
                }),
            _ => None,
        }
    }
}

impl ToGamepadButton for sdl3::gamepad::Button {
    fn to_gamepad_button(&self) -> Option<GamepadButton> {
        use sdl3::gamepad::Button::*;
        match self {
            South => Some(GamepadButton::South),
            East => Some(GamepadButton::East),
            West => Some(GamepadButton::West),
            North => Some(GamepadButton::North),

            Back => Some(GamepadButton::Back),

            Start => Some(GamepadButton::Start),
            Guide => Some(GamepadButton::Guide),

            LeftStick => Some(GamepadButton::LeftStick),
            RightStick => Some(GamepadButton::RightStick),
            LeftShoulder => Some(GamepadButton::LeftShoulder),
            RightShoulder => Some(GamepadButton::RightShoulder),

            DPadUp => Some(GamepadButton::DPadUp),
            DPadDown => Some(GamepadButton::DPadDown),
            DPadLeft => Some(GamepadButton::DPadLeft),
            DPadRight => Some(GamepadButton::DPadRight),

            Touchpad => Some(GamepadButton::Touchpad),

            LeftPaddle1 => Some(GamepadButton::LeftPaddle1),
            RightPaddle1 => Some(GamepadButton::RightPaddle1),
            LeftPaddle2 => Some(GamepadButton::LeftPaddle2),
            RightPaddle2 => Some(GamepadButton::RightPaddle2),

            Misc1 => Some(GamepadButton::Misc1),
            Misc2 => Some(GamepadButton::Misc2),
            Misc3 => Some(GamepadButton::Misc3),
            Misc4 => Some(GamepadButton::Misc4),
            Misc5 => Some(GamepadButton::Misc5),
            Misc6 => Some(GamepadButton::Misc6),
        }
    }
}
