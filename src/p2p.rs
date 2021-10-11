use ringbuf::{Consumer, RingBuffer};

use crate::discovery::{Node, Room};
use crate::peer::{Peer, ChannelWriter};
use crate::peer::PeerState;

use std::{
    collections::HashMap,
    sync::Arc,
    net::SocketAddr
};

use libp2p::{PeerId, bytes::Bytes};
use ggrs::{NonBlockingSocket, P2PSession, PlayerHandle, PlayerType, UdpMessage};

use tokio::sync::Mutex;

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
    
        println!("All connected! Let's go!!!");
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

pub(crate) struct RoomNonBlockingSocket {
    writers: HashMap<SocketAddr, ChannelWriter>,
    consumer: Consumer<(SocketAddr, Bytes)>
}

impl RoomNonBlockingSocket {
    pub(crate) async fn new(peers: Vec<Arc<Peer>>) -> Self {
        let mut writers = HashMap::new();
        let mut readers = HashMap::new();

        for peer in peers {
            let channel = &peer.channel;
            writers.insert(channel.fake_addr, channel.writer.clone());
            readers.insert(channel.fake_addr, channel.reader.clone());
        }

        let buffer = RingBuffer::new(100);
        let (producer, consumer) = buffer.split();
        let producer = Arc::new(Mutex::new(producer));
        for (addr, reader) in readers {
            let producer = producer.clone();
            tokio::spawn(async move {
                while let Some(d) = reader.lock().await.recv().await {
                    producer.lock().await.push((addr, d)).unwrap();
                }
            });
        }

        Self {
            writers,
            consumer
        }
    }
}

impl NonBlockingSocket for RoomNonBlockingSocket {
    fn send_to(&mut self, msg: &UdpMessage, addr: SocketAddr) {
        let writer = self.writers.get(&addr).unwrap();

        let buf = bincode::serialize(&msg).unwrap();
        let bytes = Bytes::from(buf);
        //println!("SEND {:?}", bytes);
        match writer.try_send(bytes) {
            Ok(_) => { },
            Err(e) => { println!("Failed to send, {:?}", e) }
        }
    }

    fn receive_all_messages(&mut self) -> Vec<(SocketAddr, UdpMessage)> {
        let mut res = Vec::new();
        while let Some((addr, bytes)) = self.consumer.pop() {
            let msg: UdpMessage = bincode::deserialize(bytes.as_ref()).unwrap();
            res.push((addr, msg));
        }

        return res;
    }
}

impl core::fmt::Debug for RoomNonBlockingSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoomNonBlockingSocket").finish()
    }
}