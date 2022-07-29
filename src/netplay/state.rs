use matchbox_socket::WebRtcSocket;
use tokio::task::JoinHandle;

use crate::settings::MAX_PLAYERS;

use super::{NetplaySession, StaticNetplayServerConfiguration, GGRSConfiguration};

pub struct InputMapping {
    pub ids: [usize; MAX_PLAYERS],
}

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

pub enum ConnectingState {
    //Load a server config (either static or through turn-on)
    LoadingNetplayServerConfiguration(JoinHandle<StaticNetplayServerConfiguration>),
    //Connecting all peers
    PeeringUp(Option<WebRtcSocket>, GGRSConfiguration),
}

#[allow(clippy::large_enum_variant)]
pub enum NetplayState {
    Disconnected,
    Connecting(StartMethod, ConnectingState),
    Connected(NetplaySession, ConnectedState)
}
