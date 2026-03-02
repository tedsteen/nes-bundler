use std::sync::OnceLock;
use crate::bundle::{BuildConfiguration, Bundle};
use crate::settings::SettingsStore;

pub struct AppContext {
    bundle: &'static Bundle,
    settings: &'static SettingsStore,
}

impl AppContext {
    pub fn global() -> &'static Self {
        static MEM: OnceLock<AppContext> = OnceLock::new();
        MEM.get_or_init(|| Self {
            bundle: Bundle::current(),
            settings: SettingsStore::global(),
        })
    }

    pub fn config(&self) -> &BuildConfiguration {
        &self.bundle.config
    }

    pub fn settings(&self) -> &'static SettingsStore {
        self.settings
    }
}
