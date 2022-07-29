use matchbox_socket::WebRtcSocket;
use tokio::task::JoinHandle;

use crate::settings::MAX_PLAYERS;

use super::{NetplaySession, StaticNetplayServerConfiguration, GGRSConfiguration};

pub struct InputMapping {
    pub ids: [usize; MAX_PLAYERS],
}

#[derive(Clone)]
pub enum StartMethod {
    Create(String),
    //Resume(SavedNetplaySession),
    Random,
}

pub enum ConnectedState {
    //Mapping netplay input
    MappingInput,
    //Playing
    Playing(InputMapping),
}

#[allow(clippy::large_enum_variant)]
pub enum NetplayState {
    Disconnected,
    //Load a server config (either static or through turn-on)
    LoadingNetplayServerConfiguration(StartMethod, JoinHandle<StaticNetplayServerConfiguration>),
    //Connecting all peers
    PeeringUp(StartMethod, Option<WebRtcSocket>, GGRSConfiguration),
    //Mapping players to inputs
    Connected(NetplaySession, ConnectedState)
}
