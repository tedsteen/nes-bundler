use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex, 
        atomic::{AtomicU16, Ordering}
    }
};

use anyhow::Result;
use webrtc_data::data_channel::DataChannel;
use libp2p::{PeerId, kad::{PeerRecord, Record}};
use webrtc::{api::{APIBuilder, setting_engine::SettingEngine}, data::data_channel::{RTCDataChannel}, peer::{
        configuration::RTCConfiguration,
        ice::{
            ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
            ice_server::RTCIceServer
        },
        peer_connection::RTCPeerConnection,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription
    }};

use crate::{discovery::{Node, PREFIX}};

#[derive(Debug)]
pub(crate) struct Channel {
    pub(crate) data_channel: Arc<DataChannel>,
    pub(crate) fake_addr: SocketAddr
}

#[derive(Clone, Debug)]
pub(crate) enum PeerState {
    Initializing,
    Connecting,
    Connected,
    Disconnected
}

#[derive(Debug)]
pub struct Peer {
    pub(crate) id: PeerId,
    pub(crate) state: Arc<Mutex<PeerState>>,
    pub(crate) channel: Channel
}
static PEER_COUNTER: AtomicU16 = AtomicU16::new(0);

impl Peer {
    pub(crate) async fn new(id: PeerId, node: Node) -> Self {
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                //TODO: add ICE-server?
                urls: vec![
                    "stun:stun.l.google.com:19302".to_owned(),
                    "stun:stun1.l.google.com:19302".to_owned(),
                    "stun:stun2.l.google.com:19302".to_owned(),
                    "stun:stun3.l.google.com:19302".to_owned(),
                    "stun:stun4.l.google.com:19302".to_owned()
                    ],
                ..Default::default()
            }],
            ..Default::default()
        };
        
        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let api = APIBuilder::new()
        .with_setting_engine(s)
        .build();
        
        let connection = Arc::new(api.new_peer_connection(config).await.unwrap());
        
        let state = Arc::new(Mutex::new(PeerState::Initializing));
        connection.on_peer_connection_state_change(Box::new({
            let state = state.clone();
            move |s: RTCPeerConnectionState| {
                let mut state = state.lock().unwrap();
                match s {
                    RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => {
                        *state = PeerState::Disconnected;
                    },
                    RTCPeerConnectionState::Connected => *state = PeerState::Connected,
                    RTCPeerConnectionState::Connecting => *state = PeerState::Connecting,
                    RTCPeerConnectionState::Unspecified => {},
                    RTCPeerConnectionState::New => {},
                };
                Box::pin(async {})
            }
        }))
        .await;

        let local_id = node.local_peer_id.clone();
        let data_channel = if local_id > id {
            Peer::do_offer(id, local_id, connection.clone(), node).await.unwrap()
        } else {
            Peer::do_answer(id, local_id, connection.clone(), node).await.unwrap()
        };
        
        PEER_COUNTER.fetch_add(1, Ordering::SeqCst);
        // TODO: Move fake addr stuff out of here (to slot logic?)
        let channel = Channel {data_channel, fake_addr: format!("127.0.0.1:{}", PEER_COUNTER.load(Ordering::SeqCst)).parse().unwrap()};

        Self {
            id,
            state,
            channel
        }
    }

    async fn do_offer(id: PeerId, local_id: PeerId, peer_connection: Arc<RTCPeerConnection>, node: Node) -> Result<Arc<DataChannel>> {
        //println!("OFFER");
        let data_channel = peer_connection.create_data_channel("data", None).await?;
        
        // Create an offer to send to the other process
        let session_description = peer_connection.create_offer(None).await?;
        peer_connection.set_local_description(session_description.clone()).await?;

        let ice_candidates = Peer::gather_candidates(peer_connection.clone()).await;
        
        let offer = Signal { session_description, ice_candidates };
        Peer::put_signal(node.clone(), local_id, id, bincode::serialize(&offer).unwrap()).await.unwrap();

        let remote_offer = Peer::get_signal(id, local_id, node).await.unwrap();
        peer_connection.set_remote_description(remote_offer.session_description).await?;
        for candidate in remote_offer.ice_candidates {
            if let Err(err) = peer_connection.add_ice_candidate(RTCIceCandidateInit {
                candidate: candidate.to_json().await?.candidate,
                ..Default::default()
            }).await {
                panic!("{}", err);
            }
        }

        // Detach data_channel
        let (tx, rx) = tokio::sync::oneshot::channel();
        data_channel.clone().on_open(Box::new(move || {
            Box::pin(async move {
                let _ = tx.send(data_channel.detach().await.unwrap());
            })
        }))
        .await;
        let data_channel = rx.await.unwrap();

        Ok(data_channel)
    }

    async fn do_answer(id: PeerId, local_id: PeerId, peer_connection: Arc<RTCPeerConnection>, node: Node) -> Result<Arc<DataChannel>> {
        //println!("ANSWER");
        let remote_offer = Peer::get_signal(id, local_id, node.clone()).await.unwrap();
        peer_connection.set_remote_description(remote_offer.session_description).await?;

        for candidate in remote_offer.ice_candidates {
            if let Err(err) = peer_connection.add_ice_candidate(RTCIceCandidateInit {
                candidate: candidate.to_json().await?.candidate,
                ..Default::default()
            }).await {
                panic!("{}", err);
            }
        }

        let session_description = peer_connection.create_answer(None).await?;
        peer_connection.set_local_description(session_description.clone()).await?;

        let ice_candidates = Peer::gather_candidates(peer_connection.clone()).await;

        let offer = Signal {session_description, ice_candidates };
        
        Peer::put_signal(node, local_id, id, bincode::serialize(&offer).unwrap()).await.unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        peer_connection.on_data_channel(Box::new(move |data_channel: Arc<RTCDataChannel>| {
            let tx = tx.clone();
            Box::pin(async move {
                // Detach data_channel
                data_channel.clone().on_open(Box::new(move || {
                    Box::pin(async move {
                        let _ = tx.send(data_channel.detach().await.unwrap()).await;
                    })
                }))
                .await;
            })
        })).await;
        let data_channel = rx.recv().await.unwrap();
        Ok(data_channel)
    }

    async fn put_signal(node: Node, from_peer: PeerId, to_peer: PeerId, offer: Vec<u8>) -> Result<(), String> {
        let key = SignalKey { from_peer, to_peer };
        let record = Record {
            key: key.to_key(),
            value: offer,
            publisher: Some(from_peer),
            expires: None,
        };
        node.put_record(record).await.map(|_| ())
    }

    async fn get_signal(from_id: PeerId, to_id: PeerId, node: Node) -> Result<Signal, String> {
        loop {
            let key = SignalKey { from_peer: from_id, to_peer: to_id };
            let res = node.get_record(key.to_key()).await;
            match res {
                Ok(ok) => {
                    let mut r = None;
                    for record in ok.records {
                        r = Some(record);
                    }
                    //TODO: when getting many like this, which one to use?
                    if let Some(PeerRecord { record, ..}) = r {
                        let ret = record.value;
                        break Ok(bincode::deserialize(&ret).unwrap());
                    };
                },
                _ => ()
            }
            // Nothing yet? Sleep and then retry...
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    
    async fn gather_candidates(peer_connection: Arc<RTCPeerConnection>) -> Vec<RTCIceCandidate> {
        println!("Gather candidates...");
        let (gather_finished_tx, mut gather_finished_rx) = tokio::sync::mpsc::channel::<()>(1);
        let mut gather_finished_tx = Some(gather_finished_tx);
        let candidates = Arc::new(Mutex::new(vec![]));

        peer_connection
        .on_ice_candidate(Box::new(
            {
                let candidates = candidates.clone(); move |c: Option<RTCIceCandidate>| {
                if let Some(candidate) = c {
                    candidates.lock().unwrap().push(candidate);
                } else {
                    gather_finished_tx.take();
                }
                Box::pin(async {})
            }
        })).await;

        let _ = gather_finished_rx.recv().await;
        let x = candidates.lock().unwrap().to_owned();
        println!("Got candidates!");

        x
    }
}

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Signal {
    ice_candidates: Vec<RTCIceCandidate>,
    session_description: RTCSessionDescription,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SignalKey {
    pub(crate) from_peer: PeerId,
    pub(crate) to_peer: PeerId
}

use libp2p::kad::record::Key;
impl SignalKey {
    fn to_key(self: &Self) -> Key {
        Key::new(&format!("{}.{}.{}", PREFIX, self.from_peer, self.to_peer))
    }
}
