use crate::input::keyboard::{JoypadKeyboardInput, DEFAULT_KEYBOARD_MAPPINGS};


pub(crate) const MAX_PLAYERS: usize = 2;
pub(crate) enum SelectedInput {
    Keyboard(JoypadKeyboardInput),
    #[allow(dead_code)] //It's coming..
    Controller(usize)
}

pub(crate) struct Settings {
    pub(crate) audio_latency: u16,
    pub(crate) inputs: [SelectedInput; MAX_PLAYERS],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_latency: 30,
            inputs:
            [
                SelectedInput::Keyboard(JoypadKeyboardInput::new(DEFAULT_KEYBOARD_MAPPINGS[0])),
                SelectedInput::Keyboard(JoypadKeyboardInput::new(DEFAULT_KEYBOARD_MAPPINGS[1])),
            ],
        }
    }
}