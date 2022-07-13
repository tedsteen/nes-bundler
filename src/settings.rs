use crate::{
    input::{JoypadInput, JoypadKeyboardInput, JoypadKeyMap}
};

pub(crate) const MAX_PLAYERS: usize = 2;
pub(crate) enum SelectedInput {
    Keyboard,
}

pub(crate) struct JoypadInputs {
    pub(crate) selected: SelectedInput,
    pub(crate) keyboard: JoypadKeyboardInput,
}

impl JoypadInputs {
    pub(crate) fn get_pad(&self) -> &dyn JoypadInput {
        match self.selected {
            SelectedInput::Keyboard => &self.keyboard,
        }
    }
}

pub(crate) struct Settings {
    pub(crate) audio_latency: u16,
    pub(crate) inputs: [JoypadInputs; MAX_PLAYERS],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_latency: 30,
            inputs:
            [
                JoypadInputs {
                    selected: SelectedInput::Keyboard,
                    keyboard: JoypadKeyboardInput::new(JoypadKeyMap::default_pad1()),
                },
                JoypadInputs {
                    selected: SelectedInput::Keyboard,
                    keyboard: JoypadKeyboardInput::new(JoypadKeyMap::default_pad2()),
                }
            ],
        }
    }
}