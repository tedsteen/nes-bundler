use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex, 
        atomic::{AtomicU16, Ordering}
    }
};

use anyhow::Result;
use libp2p::{PeerId, bytes::Bytes};
use webrtc::{
    api::{APIBuilder},
    data::data_channel::{RTCDataChannel, data_channel_message::DataChannelMessage},
    peer::{
        configuration::RTCConfiguration,
        ice::{
            ice_candidate::{RTCIceCandidate, RTCIceCandidateInit},
            ice_server::RTCIceServer
        },
        peer_connection::RTCPeerConnection,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription
    }
};

use crate::{discovery::{Node}};

pub(crate) type ChannelWriter = Arc<tokio::sync::mpsc::Sender<Bytes>>;
pub(crate) type ChannelReader = Arc<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<Bytes>>>;

#[derive(Debug)]
pub(crate) struct Channel {
    pub(crate) reader: ChannelReader,
    pub(crate) writer: ChannelWriter,
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
    pub(crate) async fn new(id: PeerId, node: Node, _room_name: &str) -> Self {
        
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

        let api = APIBuilder::new().build();
        let connection = Arc::new(api.new_peer_connection(config).await.unwrap());
        
        let (send_sender, mut send_receiver) = tokio::sync::mpsc::channel(100);
        let (recv_sender, recv_receiver) = tokio::sync::mpsc::channel(100);
        let recv_sender = Arc::new(recv_sender);

        let local_id = node.local_peer_id.clone();
        tokio::spawn({
            let connection = connection.clone();
            async move {
                let data_channel = if local_id > id {
                    Peer::do_offer(id, local_id, connection, node).await.unwrap()
                } else {
                    Peer::do_answer(id, local_id, connection, node).await.unwrap()
                };
        
                data_channel.on_open(Box::new({
                    let data_channel = data_channel.clone();
                    move || {
                        tokio::spawn({
                            let data_channel = data_channel.clone();
                            async move {
                                data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
                                    let data = msg.data;
                                    let recv_sender = recv_sender.clone();
                                    
                                    Box::pin(async move {
                                        recv_sender.send(data).await.unwrap();
                                    })
                                })).await;
                            }
                        });

                        tokio::spawn(async move {
                            while let Some(r) = send_receiver.recv().await {
                                data_channel.send(&r).await.unwrap();
                            }
                        });
                        Box::pin(async move {})
                    }
                })).await;
            }
        });

        PEER_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        let channel = Channel {reader: Arc::new(tokio::sync::Mutex::new(recv_receiver)), writer: Arc::new(send_sender), fake_addr: format!("127.0.0.1:{}", PEER_COUNTER.load(Ordering::SeqCst)).parse().unwrap()};

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

        Self {
            id,
            state,
            channel
        }
    }

    async fn do_offer(id: PeerId, local_id: PeerId, peer_connection: Arc<RTCPeerConnection>, node: Node) -> Result<Arc<RTCDataChannel>> {
        let data_channel = peer_connection.create_data_channel("data", None).await?;

        // Create an offer to send to the other process
        let session_description = peer_connection.create_offer(None).await?;
        peer_connection.set_local_description(session_description.clone()).await?;

        let ice_candidates = Peer::gather_candidates(peer_connection.clone()).await;
        
        let offer = Signal { session_description, ice_candidates };
        node.put_signal(local_id, id, bincode::serialize(&offer).unwrap()).await.unwrap();

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

        Ok(data_channel)
    }

    async fn do_answer(id: PeerId, local_id: PeerId, peer_connection: Arc<RTCPeerConnection>, node: Node) -> Result<Arc<RTCDataChannel>> {
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
        
        node.put_signal(local_id, id, bincode::serialize(&offer).unwrap()).await.unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        // Register data channel creation handling
        peer_connection.on_data_channel(Box::new(move |d: Arc<RTCDataChannel>| {
            let tx = tx.clone();
            Box::pin(async move {
                let _ = tx.send(d).await;
            })
        })).await;

        let data_channel = rx.recv().await.unwrap();
        Ok(data_channel)
    }

    async fn get_signal(from_id: PeerId, to_id: PeerId, node: Node) -> Result<Signal, String> {
        loop {
            let res = node.get_signal(from_id, to_id).await;
            match res {
                Ok(value) => {
                    return Ok(bincode::deserialize(&value).unwrap());
                },
                Err(_) => {

                    //Sleep and then retry
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                },
            }
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
