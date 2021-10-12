use tokio::runtime::{Handle};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::discovery::{Node, Room};
use crate::peer::{Peer};
use crate::peer::PeerState;

use std::{
    collections::HashMap,
    sync::Arc,
    net::SocketAddr
};

use libp2p::{PeerId, bytes::Bytes};
use ggrs::{NonBlockingSocket, P2PSession, PlayerHandle, PlayerType, UdpMessage};

pub(crate) struct P2PGame {
    pub(crate) session: P2PSession,
    pub(crate) local_handle: PlayerHandle
}

pub(crate) struct P2P {
    input_size: usize
}

impl P2P {
    pub(crate) fn new(input_size: usize) -> Self {
        Self { input_size }
    }

    pub(crate) async fn start_game(self: &Self, room: &mut Room, num_players: u16, node: Node) -> P2PGame {
        println!("Waiting for {} peers...", num_players - 1);
        let peers = loop {
            let peers = room.get_peers(node.clone()).await;
            let connected_peers: Vec<Arc<Peer>> = peers.iter().filter(|p| {
                match &*p.state.lock().unwrap() {
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

        let sock = RoomNonBlockingSocket::new(peers.clone()).await;
        let mut session = P2PSession::new_with_socket(num_players as u32, self.input_size, sock).expect("Could not create a P2P Session");

        let mut peers: Vec<PeerType> = peers.iter().map(|p| PeerType::Remote(p.clone())).collect();
        peers.push(PeerType::Local(node.local_peer_id));

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

enum PeerType {
    Local(PeerId),
    Remote(Arc<Peer>)
}

const RECV_BUFFER_SIZE: usize = 4096;
pub(crate) struct RoomNonBlockingSocket {
    runtime_handle: Handle,
    reader: Receiver<(SocketAddr, UdpMessage)>,
    sender: Sender<(SocketAddr, UdpMessage)>
}

impl RoomNonBlockingSocket {
    pub(crate) async fn new(peers: Vec<Arc<Peer>>) -> Self {
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