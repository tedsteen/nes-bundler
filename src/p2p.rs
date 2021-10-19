use tokio::runtime::{Handle};

use crate::discovery::{self, Node};
use crate::peer::{Peer, PeerState};

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::{
    collections::HashMap,
    net::SocketAddr
};

use libp2p::{PeerId, bytes::Bytes};
use ggrs::{NonBlockingSocket, P2PSession, PlayerType, UdpMessage};

#[derive(Debug, Clone)]
pub(crate) enum Participant {
    Local(PeerId),
    Remote(PeerId, SocketAddr)
}
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    Vacant(),
    Occupied(Participant)
}
pub(crate) struct ReadyState {
    pub(crate) players: Vec<Participant>,
    spectators: Vec<Participant>
}
pub(crate) enum GameState {
    Initializing,
    New(Vec<Slot>),
    Ready(ReadyState),
}
pub(crate) struct P2PGame {
    owner_id: PeerId,
    node: Node,
}

impl P2PGame {    
    async fn new(owner_id: PeerId, node: Node) -> Self {
        Self {
            owner_id,
            node,
        }
    }

    //TODO: change to u8 not usize...
    async fn get_slot_count(self: &Self) -> Option<usize> {
        self.get_record("slot-count").await.map(|slot_count_data| bincode::deserialize(&slot_count_data).unwrap())
    }

    async fn get_name(self: &Self) -> Option<String> {
        self.get_record("name").await.map(|name_data| bincode::deserialize(&name_data).unwrap())
    }

    pub(crate) async fn current_state(self: &Self) -> GameState {
        if let Some(slots) = self.get_slots().await {
            let participants = slots.iter()
            .filter_map(|slot| {
                if let Slot::Occupied(occupied_slot) = slot {
                    Some(occupied_slot.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
            if participants.len() == slots.len() {
                GameState::Ready(ReadyState { players: participants, spectators: vec!() })
            } else {
                GameState::New(slots)
            }
        } else {
            GameState::Initializing
        }
    }

    async fn get_slots(self: &Self) -> Option<Vec<Slot>> {
        if let Some(slot_count) = self.get_slot_count().await {
            let mut slots = Vec::with_capacity(slot_count);
            for idx in 0..slot_count {
                let slot = match self.get_slot_owner(idx).await {
                    Some(slot_owner) => {
                        if slot_owner == self.node.local_peer_id {
                            Slot::Occupied(Participant::Local(slot_owner))
                        } else {
                            Slot::Occupied(Participant::Remote(slot_owner, format!("127.0.0.1:{}", idx).parse().unwrap()))
                        }
                    },
                    None => Slot::Vacant(),
                };
                slots.push(slot);
            }
            Some(slots)
        } else {
            None
        }
    }

    async fn get_slot_owner(self: &Self, idx: usize) -> Option<PeerId> {
        
        let mut providers = Vec::new();
        for peer_id in self.get_providers("slot-idx").await {
            let key = format!("{}.slot-idx", peer_id);
            if let Some(slot_idx) = self.get_record(&key).await {
                let slot_idx = bincode::deserialize(&slot_idx).unwrap();
                if idx == slot_idx {
                    providers.push(peer_id);
                }
            }
        }
        
        // Pick the largest id so everyone gets the same result
        providers.iter().cloned().max()
    }

    pub(crate) async fn claim_slot(self: &Self, slot_idx: usize) {
        let key = format!("{}.slot-idx", self.node.local_peer_id);
        //println!("Claim slot {:?}, {:?}", slot_idx, key);
        self.start_providing("slot-idx").await;
        self.put_record(&key, bincode::serialize(&slot_idx).unwrap()).await;
    }

    async fn start_providing(self: &Self, key: &str) {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.start_providing(&key).await;
    }

    async fn get_providers(self: &Self, key: &str) -> HashSet<PeerId> {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.get_providers(&key).await
    }

    async fn put_record(self: &Self, key: &str, value: Vec<u8>) {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.put_record(&key, value, None).await;
    }

    async fn get_record(self: &Self, key: &str) -> Option<Vec<u8>> {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.get_record(&key).await
    }
}

#[derive(Clone)]
pub(crate) struct P2P {
    node: Node,
    peers: Arc<Mutex<HashMap<PeerId, Peer>>>,
}

impl P2P {
    pub(crate) async fn new() -> Self {
        let node = discovery::Node::new().await;
        Self {
            node,
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn get_peer(self: &Self, peer_id: PeerId) -> Peer {
        let mut lock = self.peers.lock().unwrap();
        if let Some(peer) = lock.get(&peer_id) {
            return peer.clone();
        } else {
            let peer = Peer::new(peer_id, self.node.clone()).await;
            lock.insert(peer_id, peer.clone());
            return peer;
        }
    }

    pub(crate) async fn find_owner(self: &Self, game_name: &str) -> Option<PeerId> {
        let mut owners = vec!();
        let providers = self.node.get_providers("p2p-game").await;
        for peer_id in providers {
            for name in self.node.get_record(&format!("{}.p2p-game.name", peer_id)).await {
                let name: &str = bincode::deserialize(&name).unwrap();
                if name == game_name {
                    owners.push(peer_id);
                }
            }
        }
        owners.iter().cloned().max()
    }

    pub(crate) async fn join_game(self: &Self, owner_id: PeerId) -> P2PGame {
        P2PGame::new(owner_id, self.node.clone()).await
    }

    pub(crate) async fn create_game(self: &Self, name: &str, num_players: usize) -> P2PGame {
        let game = P2PGame::new(self.node.local_peer_id, self.node.clone()).await;
        self.node.start_providing("p2p-game").await;
        game.put_record("slot-count", bincode::serialize(&num_players).unwrap()).await;
        game.put_record("name", bincode::serialize(&name).unwrap()).await;
        game
    }

    pub(crate) async fn start_session(self: &Self, ready_state: ReadyState, input_size: usize) -> (P2PSession, usize) {
        let num_players = ready_state.players.len() + ready_state.spectators.len();
        println!("Players: {}", num_players);
        
        let sock = P2PGameNonBlockingSocket::new(&ready_state, &self).await;
        let mut session = P2PSession::new_with_socket(num_players as u32, input_size, sock).expect("Could not create a P2P Session");

        let local_handle = {
            let mut local_handle = 0;
            for (slot_idx, slot) in ready_state.players.iter().enumerate() {
                match slot {
                    Participant::Local(_) => {
                        println!("Add local player {}", slot_idx);
                        local_handle = session.add_player(PlayerType::Local, slot_idx).unwrap();
                    },
                    Participant::Remote(_, addr) => {
                        println!("Add remote player {:?}", addr);
                        session.add_player(PlayerType::Remote(addr.clone()), slot_idx).unwrap();
                    },
                }
            }
            local_handle
        };
        (session, local_handle)
    }
}

const RECV_BUFFER_SIZE: usize = 4096;
pub(crate) struct P2PGameNonBlockingSocket {
    runtime_handle: Handle,
    reader: tokio::sync::mpsc::Receiver<(std::net::SocketAddr, UdpMessage)>,
    sender: tokio::sync::mpsc::Sender<(std::net::SocketAddr, UdpMessage)>
}

impl P2PGameNonBlockingSocket {
    pub(crate) async fn new(ready_state: &ReadyState, p2p: &P2P) -> Self {
        let runtime_handle = tokio::runtime::Handle::current();
        let participants = [&ready_state.players[..], &ready_state.spectators[..]].concat();

        let mut peers = HashMap::new();
        for participant in participants {
            match participant {
                Participant::Remote(peer_id, addr) => {
                    let peer = p2p.get_peer(peer_id).await;
                    peers.insert(addr.clone(), (peer, [0; RECV_BUFFER_SIZE]));
                },
                _ => ()
            }
        }

        // Read loop
        let (tx, reader) = tokio::sync::mpsc::channel(100);
        for (src_addr, (peer, mut buffer)) in peers.clone() {
            runtime_handle.spawn({
                let tx = tx.clone();
                async move {
                    //println!("Starting read loop for {:?}", src_addr);
                    loop {
                        let peer_state = peer.connection_state.borrow().clone();
                        match peer_state {
                            PeerState::Connected(channel) => {
                                while let Ok(number_of_bytes) = channel.read(&mut buffer).await {
                                    assert!(number_of_bytes <= RECV_BUFFER_SIZE);
                                    if let Ok(msg) = bincode::deserialize::<UdpMessage>(&buffer[0..number_of_bytes]) {
                                        //println!("READ: {:?} - {:?}", msg, src_addr);
                                        tx.send((src_addr.clone(), msg)).await.unwrap();
                                    } else {
                                        eprintln!("Failed to deserialize message, message discarded");
                                    }
                                }
                                println!("Exited read loop for {:?}", src_addr);
                            },
                            //TODO: exit when the peer state is unrecoverable
                            _ => eprintln!("Peer {:?} not in a state where it can read. Will try again...", src_addr),
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            });
        };

        // Write loop
        let (sender, mut rx) = tokio::sync::mpsc::channel::<(SocketAddr, UdpMessage)>(100);
        runtime_handle.spawn({
            async move {
                while let Some((addr, msg)) = rx.recv().await {
                    let (peer, _) = peers.get(&addr).unwrap();
                    let peer_state = peer.connection_state.borrow().clone();
                    if let PeerState::Connected(channel) = peer_state {
                        let buf = bincode::serialize(&msg).unwrap();
                        let bytes = Bytes::from(buf);
                        channel.write(&bytes).await.unwrap();
                        //println!("SEND: {:?} - {:?}", msg, addr);    
                    } else {
                        eprintln!("Peer {:?} was not in a state where it could write. Message discarded.", addr);
                    }
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

impl NonBlockingSocket for P2PGameNonBlockingSocket {
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

impl core::fmt::Debug for P2PGameNonBlockingSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("P2PGameNonBlockingSocket").finish()
    }
}