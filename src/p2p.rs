use libp2p::kad::GetProvidersOk;
use libp2p::kad::record::Key;
use tokio::runtime::{Handle};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::discovery::{self, Node, PREFIX};
use crate::peer::{Peer};
use crate::peer::PeerState;

use std::collections::HashSet;
use std::{
    collections::HashMap,
    net::SocketAddr
};

use libp2p::{PeerId, bytes::Bytes};
use ggrs::{NonBlockingSocket, P2PSession, PlayerHandle, PlayerType, UdpMessage};

pub(crate) struct P2PGame {
    pub(crate) session: P2PSession,
    pub(crate) local_handle: PlayerHandle
}

pub(crate) struct P2P {
    input_size: usize,
    node: Node
}

impl P2P {
    pub(crate) async fn new(input_size: usize) -> Self {
        //let runtime = tokio::runtime::Builder::new_multi_thread()
        //.enable_all()
        //.build().unwrap();

        //let node = runtime.block_on(async {
        let node = discovery::Node::new().await;
        //});

        Self { input_size, node }
    }

    pub(crate) async fn start_game(self: &Self, num_players: u16) -> P2PGame {
        let mut room = Room::new(&String::from("private"), self.node.clone()).await;

        println!("Waiting for {} peers...", num_players - 1);
        let peers = loop {
            let peers = room.get_peers().await;
            let connected_peers: Vec<Peer> = peers.iter().filter(|&peer| {
                match peer.get_state() {
                    PeerState::Connected => true,
                    _ => false,
                }
            }).cloned()
            .collect();

            if connected_peers.len() == (num_players - 1) as usize {
                break connected_peers;
            } else {
                println!("Connecting: {}, Connected: {}", peers.len() - connected_peers.len(), connected_peers.len());
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
        };
        println!("All connected!");

        let sock = RoomNonBlockingSocket::new(peers.clone()).await;
        let mut session = P2PSession::new_with_socket(num_players as u32, self.input_size, sock).expect("Could not create a P2P Session");

        let mut peers: Vec<PeerType> = peers.iter().map(|p| PeerType::Remote(p.clone())).collect();
        peers.push(PeerType::Local(self.node.local_peer_id));

        peers.sort_by(|a, b| {
            let a = match a {
                PeerType::Local(peer_id) => *peer_id,
                PeerType::Remote(peer) => peer.id.clone(),
            };
            let b = match b {
                PeerType::Local(peer_id) => *peer_id,
                PeerType::Remote(peer) => peer.id.clone(),
            };
            a.cmp(&b)
        });

        let local_handle = {
            let mut peer_count = 0;
            let mut local_handle = 0;
            for peer_type in peers {
                match peer_type {
                    PeerType::Local(_) => {
                        local_handle = session.add_player(PlayerType::Local, peer_count).unwrap();
                    },
                    PeerType::Remote(peer) => {
                        session.add_player(PlayerType::Remote(peer.channel.fake_addr), peer_count).unwrap();
                    }
                }
                peer_count += 1;
            }
            local_handle
        };
        P2PGame { session, local_handle }
    }
}

pub(crate) struct Room {
    node: Node,
    name: String,
    peers: HashMap<PeerId, Peer>
}

impl Room {
    pub(crate) async fn new(name: &str, node: Node) -> Self {
        match node.start_providing(Key::new(&format!("{}.{}.signal_data", PREFIX, name))).await {
            Ok(_ok) => {},
            Err(err) => panic!("Failed to provide signal {:?}", err),
        }

        Self {
            node,
            name: name.to_string(),
            peers: HashMap::new()
        }
    }

    pub(crate) async fn get_peers(self: &mut Self) -> Vec<Peer> {    
        let peer_ids = self.get_peer_ids(self.node.clone()).await.unwrap();

        for peer_id in peer_ids {
            let peer = self.peers.entry(peer_id);
            match peer {
                std::collections::hash_map::Entry::Occupied(_) => {},
                std::collections::hash_map::Entry::Vacant(_) => {
                    self.peers.insert(peer_id, Peer::new(peer_id, self.node.clone()).await);
                },
            }
        }

        self.peers.values().cloned().collect()
    }

    async fn get_peer_ids(self: &Self, node: Node) -> Result<HashSet<PeerId>, String> {
        let room_name = self.name.clone();

        match node.get_providers(Key::new(&format!("{}.{}.signal_data", PREFIX, room_name))).await {
            Ok(GetProvidersOk { providers, ..}) => Ok(providers),
            Err(e) => Err(e),
        }
    }
}

enum PeerType {
    Local(PeerId),
    Remote(Peer)
}

const RECV_BUFFER_SIZE: usize = 4096;
pub(crate) struct RoomNonBlockingSocket {
    runtime_handle: Handle,
    reader: Receiver<(SocketAddr, UdpMessage)>,
    sender: Sender<(SocketAddr, UdpMessage)>
}

impl RoomNonBlockingSocket {
    pub(crate) async fn new(peers: Vec<Peer>) -> Self {
        let runtime_handle = tokio::runtime::Handle::current();
        let mut channels = HashMap::with_capacity(peers.len());
        for peer in peers {
            let channel = &peer.channel;
            channels.insert(channel.fake_addr, (channel.data_channel.clone(), [0; RECV_BUFFER_SIZE]));
        }

        // Read loop
        let (tx, reader) = tokio::sync::mpsc::channel(100);
        for (src_addr, (channel, mut buffer)) in channels.clone() {
            runtime_handle.spawn({
                let tx = tx.clone();
                async move {
                    //println!("Starting read loop for {:?}", src_addr);
                    while let Ok(number_of_bytes) = channel.read(&mut buffer).await {
                        assert!(number_of_bytes <= RECV_BUFFER_SIZE);
                        if let Ok(msg) = bincode::deserialize::<UdpMessage>(&buffer[0..number_of_bytes]) {
                            //println!("READ: {:?} - {:?}", msg, src_addr);
                            tx.send((src_addr, msg)).await.unwrap();
                        } else {
                            eprintln!("Failed to deserialize message, message discarded");
                        }
                    }
                    println!("Exited read loop for {:?}", src_addr);
                }
            });
        };

        // Write loop
        let (sender, mut rx) = tokio::sync::mpsc::channel::<(SocketAddr, UdpMessage)>(100);
        runtime_handle.spawn({
            async move {
                //println!("Starting write loop");
                while let Some((addr, msg)) = rx.recv().await {
                    let (channel, _) = channels.get(&addr).unwrap();
                    let buf = bincode::serialize(&msg).unwrap();
                    let bytes = Bytes::from(buf);
                    channel.write(&bytes).await.unwrap();
                    //println!("SEND: {:?} - {:?}", msg, addr);
                }
                println!("Exited write loop");
            }
        });

        Self {
            runtime_handle,
            reader,
            sender
        }
    }
}

impl NonBlockingSocket for RoomNonBlockingSocket {
    fn send_to(&mut self, msg: &UdpMessage, addr: SocketAddr) {
        let msg = msg.clone();
        let sender = self.sender.clone();
        self.runtime_handle.spawn(async move {
            sender.send((addr, msg)).await.unwrap();
        });
    }

    fn receive_all_messages(&mut self) -> Vec<(SocketAddr, UdpMessage)> {
        let mut messages = Vec::new();
        while let Ok(msg) = self.reader.try_recv() {
            messages.push(msg);
        }
        messages
    }
}

impl core::fmt::Debug for RoomNonBlockingSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomNonBlockingSocket").finish()
    }
}