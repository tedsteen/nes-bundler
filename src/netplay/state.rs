use std::time::Instant;

use matchbox_socket::WebRtcSocket;
use serde::Deserialize;
use tokio::task::JoinHandle;

use crate::settings::MAX_PLAYERS;

use super::{NetplaySession, GGRSConfiguration, TurnOnResponse};

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
    pub start_time: Instant,
}
impl PeeringState {
    pub fn new(socket: Option<WebRtcSocket>, ggrs_config: GGRSConfiguration, unlock_url: Option<String>) -> Self {
        let start_time = Instant::now();
        PeeringState { socket, ggrs_config, unlock_url, start_time }
    }
}
pub enum ConnectingState {
    //Load a server config
    LoadingNetplayServerConfiguration(JoinHandle<Result<TurnOnResponse, TurnOnError>>),
    //Connecting all peers
    PeeringUp(PeeringState),
}

#[allow(clippy::large_enum_variant)]
pub enum NetplayState {
    Disconnected,
    Connecting(StartMethod, ConnectingState),
    Connected(NetplaySession, ConnectedState)
}
