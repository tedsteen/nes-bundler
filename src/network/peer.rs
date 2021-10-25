use std::sync::Arc;

use anyhow::Result;
use libp2p::PeerId;
use tokio::sync::watch::Receiver;
use webrtc::{
    api::{setting_engine::SettingEngine, APIBuilder},
    data::data_channel::RTCDataChannel,
    peer::{
        configuration::RTCConfiguration,
        ice::{
            ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
            ice_server::RTCIceServer,
        },
        peer_connection::RTCPeerConnection,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
};
use webrtc_data::data_channel::DataChannel;

use crate::network::discovery::Node;

#[derive(Clone)]
pub(crate) enum PeerState {
    Initializing,
    Connecting,
    Connected(Arc<DataChannel>),
    Disconnected, // TODO: Come up with a state when a peer is truly dead and should be discarded (so we can stop read/write loops)
}

impl std::fmt::Debug for PeerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initializing => write!(f, "Initializing"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected(_) => f.debug_tuple("Connected").finish(),
            Self::Disconnected => write!(f, "Disconnected"),
        }
    }
}

#[derive(Clone)]
pub struct Peer {
    pub(crate) id: PeerId,
    pub(crate) connection_state: Receiver<PeerState>,
    node: Node,
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peer")
            .field("id", &self.id)
            .field("connection_state", &*self.connection_state.borrow())
            .finish()
    }
}

impl Peer {
    pub(crate) fn new(id: PeerId, node: &Node) -> Self {
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                //TODO: add ICE-server?
                urls: vec![
                    "stun:stun.l.google.com:19302".to_owned(),
                    "stun:stun1.l.google.com:19302".to_owned(),
                    "stun:stun2.l.google.com:19302".to_owned(),
                    "stun:stun3.l.google.com:19302".to_owned(),
                    "stun:stun4.l.google.com:19302".to_owned(),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let api = Arc::new(APIBuilder::new().with_setting_engine(s).build());

        let (connection_state_sender, connection_state) =
            tokio::sync::watch::channel(PeerState::Initializing);

        // Connect and start watching the connection state.
        tokio::spawn({
            let node = node.clone();
            async move {
                println!("Connecting new peer: {:?}", id);
                let connection = &api.new_peer_connection(config).await.unwrap();
                let (a, mut b) = tokio::sync::watch::channel(RTCPeerConnectionState::Unspecified);

                connection
                    .on_peer_connection_state_change(Box::new({
                        move |state: RTCPeerConnectionState| {
                            a.send(state).unwrap();
                            Box::pin(async move {})
                        }
                    }))
                    .await;

                let local_id = node.local_peer_id;
                let data_channel = if local_id > id {
                    Peer::do_offer(id, local_id, connection, &node)
                        .await
                        .unwrap()
                } else {
                    Peer::do_answer(id, local_id, connection, &node)
                        .await
                        .unwrap()
                };
                println!("Connected! {:?}", id);

                loop {
                    let new_state = match *b.borrow() {
                        RTCPeerConnectionState::Disconnected
                        | RTCPeerConnectionState::Failed
                        | RTCPeerConnectionState::Closed => Some(PeerState::Disconnected),
                        RTCPeerConnectionState::Connected => {
                            Some(PeerState::Connected(data_channel.clone()))
                        }
                        RTCPeerConnectionState::Connecting => Some(PeerState::Connecting),
                        _ => None,
                    };

                    if let Some(new_state) = new_state {
                        if connection_state_sender.send(new_state).is_err() {
                            // No one is listening, break out.
                            println!("Exiting connection state loop for peer {:?}", id);
                            break;
                        }
                    }

                    if b.changed().await.is_err() {
                        break;
                    }
                }
            }
        });

        Self {
            id,
            connection_state,
            node: node.clone(),
        }
    }

    async fn do_offer(
        id: PeerId,
        local_id: PeerId,
        peer_connection: &RTCPeerConnection,
        node: &Node,
    ) -> Result<Arc<DataChannel>> {
        //println!("OFFER");
        let data_channel = peer_connection.create_data_channel("data", None).await?;

        // Create an offer to send to the other process
        let session_description = peer_connection.create_offer(None).await?;
        peer_connection
            .set_local_description(session_description.clone())
            .await?;

        let ice_candidates = Peer::gather_candidates(peer_connection).await;

        let offer = Signal {
            session_description,
            ice_candidates,
        };
        Peer::put_signal(node, local_id, id, &bincode::serialize(&offer).unwrap()).await;

        let remote_offer = Peer::get_signal(id, local_id, node).await.unwrap();
        peer_connection
            .set_remote_description(remote_offer.session_description)
            .await?;
        for candidate in remote_offer.ice_candidates {
            if let Err(err) = peer_connection
                .add_ice_candidate(RTCIceCandidateInit {
                    candidate: candidate.to_json().await?.candidate,
                    ..Default::default()
                })
                .await
            {
                panic!("{}", err);
            }
        }

        // Detach data_channel
        let (tx, rx) = tokio::sync::oneshot::channel();
        data_channel
            .clone()
            .on_open(Box::new(move || {
                Box::pin(async move {
                    let _ = tx.send(data_channel.detach().await.unwrap());
                })
            }))
            .await;
        let data_channel = rx.await.unwrap();

        Ok(data_channel)
    }

    async fn do_answer(
        id: PeerId,
        local_id: PeerId,
        peer_connection: &RTCPeerConnection,
        node: &Node,
    ) -> Result<Arc<DataChannel>> {
        //println!("ANSWER");
        let remote_offer = Peer::get_signal(id, local_id, node).await.unwrap();
        peer_connection
            .set_remote_description(remote_offer.session_description)
            .await?;

        for candidate in remote_offer.ice_candidates {
            if let Err(err) = peer_connection
                .add_ice_candidate(RTCIceCandidateInit {
                    candidate: candidate.to_json().await?.candidate,
                    ..Default::default()
                })
                .await
            {
                panic!("{}", err);
            }
        }

        let session_description = peer_connection.create_answer(None).await?;
        peer_connection
            .set_local_description(session_description.clone())
            .await?;

        let ice_candidates = Peer::gather_candidates(peer_connection).await;

        let offer = Signal {
            session_description,
            ice_candidates,
        };

        Peer::put_signal(node, local_id, id, &bincode::serialize(&offer).unwrap()).await;

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        peer_connection
            .on_data_channel(Box::new(move |data_channel: Arc<RTCDataChannel>| {
                let tx = tx.clone();
                Box::pin(async move {
                    // Detach data_channel
                    data_channel
                        .clone()
                        .on_open(Box::new(move || {
                            Box::pin(async move {
                                let _ = tx.send(data_channel.detach().await.unwrap()).await;
                            })
                        }))
                        .await;
                })
            }))
            .await;
        let data_channel = rx.recv().await.unwrap();
        Ok(data_channel)
    }

    async fn put_signal(node: &Node, from_peer: PeerId, to_peer: PeerId, offer: &[u8]) {
        let key = format!("{}.signal.{}", from_peer, to_peer);
        node.put_record(&key, offer.to_vec(), None).await;
    }

    async fn get_signal(from_peer: PeerId, to_peer: PeerId, node: &Node) -> Result<Signal, String> {
        let key = format!("{}.signal.{}", from_peer, to_peer);
        loop {
            if let Some(signal) = node.get_record(&key).await {
                break Ok(bincode::deserialize(&signal).unwrap());
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    async fn gather_candidates(peer_connection: &RTCPeerConnection) -> Vec<RTCIceCandidate> {
        //println!("Gather candidates...");
        let mut gather_complete = peer_connection.gathering_complete_promise().await;
        let (candidate_sender, mut candidate_receiver) = tokio::sync::mpsc::unbounded_channel();

        peer_connection
            .on_ice_candidate(Box::new({
                move |c: Option<RTCIceCandidate>| {
                    if let Some(candidate) = c {
                        let _ = candidate_sender.send(candidate);
                    }
                    Box::pin(async move {})
                }
            }))
            .await;

        let _ = gather_complete.recv().await;
        let mut candidates = vec![];
        while let Ok(m) = candidate_receiver.try_recv() {
            candidates.push(m);
        }
        println!("Got {} candidates!", candidates.len());
        candidates
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Signal {
    ice_candidates: Vec<RTCIceCandidate>,
    session_description: RTCSessionDescription,
}

#[derive(Clone, Debug, PartialEq)]
struct SignalKey {
    from_peer: PeerId,
    to_peer: PeerId,
}
