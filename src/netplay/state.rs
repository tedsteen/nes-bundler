use std::time::Instant;

use futures::channel::oneshot::Receiver;
use ggrs::P2PSession;
use matchbox_socket::WebRtcSocket;
use serde::Deserialize;

use crate::settings::MAX_PLAYERS;

use super::{NetplaySession, GGRSConfiguration, TurnOnResponse, GGRSConfig};

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

#[derive(Deserialize, Debug)]
pub struct TurnOnError {
    pub description: String,
}

pub struct PeeringState {
    pub socket: Option<WebRtcSocket>,
    pub ggrs_config: GGRSConfiguration,
    pub unlock_url: Option<String>,
}
impl PeeringState {
    pub fn new(socket: Option<WebRtcSocket>, ggrs_config: GGRSConfiguration, unlock_url: Option<String>) -> Self {
        PeeringState { socket, ggrs_config, unlock_url }
    }
}

pub struct SynchonizingState {
    pub p2p_session: Option<P2PSession<GGRSConfig>>,
    pub unlock_url: Option<String>,
    pub start_time: Instant,
}
impl SynchonizingState {
    pub fn new(p2p_session: Option<P2PSession<GGRSConfig>>, unlock_url: Option<String>) -> Self {
        let start_time = Instant::now();
        SynchonizingState { p2p_session, unlock_url, start_time }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum ConnectingState {
    //Load a server config
    LoadingNetplayServerConfiguration(Receiver<Result<TurnOnResponse, TurnOnError>>),
    //Connecting all peers
    PeeringUp(PeeringState),
    Synchronizing(SynchonizingState)
}

#[allow(clippy::large_enum_variant)]
pub enum NetplayState {
    Disconnected,
    Connecting(StartMethod, ConnectingState),
    Connected(NetplaySession, ConnectedState)
}
