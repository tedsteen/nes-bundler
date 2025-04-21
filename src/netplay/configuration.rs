use serde::Deserialize;

use crate::settings::Settings;

#[derive(Deserialize, Clone, Debug)]
pub struct GGRSConfiguration {
    pub max_prediction: usize,
    pub input_delay: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct IcePasswordCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize, Clone, Debug)]
pub enum IceCredentials {
    None,
    Password(IcePasswordCredentials),
}

#[derive(Deserialize, Clone, Debug)]
pub struct IceConfiguration {
    pub urls: Vec<String>,
    pub credentials: IceCredentials,
}

#[derive(Deserialize, Clone, Debug)]
pub struct MatchboxConfiguration {
    pub server: String,
    pub ice: IceConfiguration,
}

#[derive(Deserialize, Clone, Debug)]
pub struct StaticNetplayServerConfiguration {
    pub matchbox: MatchboxConfiguration,
    pub ggrs: GGRSConfiguration,
    pub unlock_url: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TurnOnServerConfiguration {
    pub url: String,
    pub netplay_id: Option<String>,
}

impl TurnOnServerConfiguration {
    pub fn get_netplay_id(&self) -> String {
        self.netplay_id
            .clone()
            .unwrap_or_else(|| Settings::current_mut().get_netplay_id())
            .to_string()
    }
}
#[derive(Deserialize, Clone, Debug)]
pub enum NetplayServerConfiguration {
    Static(StaticNetplayServerConfiguration),
    TurnOn(TurnOnServerConfiguration),
}

#[derive(Deserialize, Clone, Debug)]
pub struct NetplayBuildConfiguration {
    pub server: NetplayServerConfiguration,
}