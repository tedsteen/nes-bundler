use std::{collections::{HashMap}, rc::Rc, cell::RefCell};

use crate::input::{InputConfiguration, InputId, keyboard::{Keyboards}};

pub(crate) const MAX_PLAYERS: usize = 2;
pub(crate) type InputConfigurationRef = Rc<RefCell<InputConfiguration>>;

#[derive(Debug)]
pub(crate) struct InputSettings {
    pub(crate) selected: [InputConfigurationRef; MAX_PLAYERS],
    pub(crate) configurations: HashMap<InputId, InputConfigurationRef>
}
#[derive(Debug)]
pub(crate) struct AudioSettings {
    pub(crate) latency: u16
}

#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) audio: AudioSettings,
    pub(crate) input: InputSettings
}

impl InputSettings {
    pub(crate) fn get_or_create_config(&mut self, id: &InputId, default: InputConfiguration) -> &InputConfigurationRef {
        self.configurations.entry(id.clone()).or_insert_with(|| Rc::new(RefCell::new(default)))
    }
    pub(crate) fn get_default_config(&mut self, player: usize) -> &InputConfigurationRef {
        let default = Keyboards::default_configurations(player);
        self.get_or_create_config(&default.id.clone(), default)
    }
}

impl Default for Settings {
    fn default() -> Self {
        let audio = AudioSettings {
            latency: 40
        };
        let default_input_1 = Rc::new(RefCell::new(Keyboards::default_configurations(0)));
        let default_input_2 = Rc::new(RefCell::new(Keyboards::default_configurations(1)));

        let mut configurations = HashMap::new();
        configurations.insert(default_input_1.borrow().id.clone(), default_input_1.clone());
        configurations.insert(default_input_2.borrow().id.clone(), default_input_2.clone());

        let selected = [
            Rc::clone(&default_input_1),
            Rc::clone(&default_input_2),
        ];

        let input = InputSettings {
            selected,
            configurations
        };

        Self { audio, input }
    }
}