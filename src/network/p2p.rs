use tokio::runtime::Handle;

use crate::network::discovery::{self, Node};
use crate::network::peer::{Peer, PeerState};

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, net::SocketAddr};

use ggrs::{NonBlockingSocket, P2PSession, PlayerType, UdpMessage};
use libp2p::{bytes::Bytes, PeerId};

#[derive(Debug, Clone)]
pub(crate) enum Participant {
    Local(PeerId),
    Remote(Peer, SocketAddr),
}
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    Vacant(),
    Occupied(Participant),
}
#[derive(Debug, Clone)]
pub(crate) struct ReadyState {
    pub(crate) players: Vec<Participant>,
    spectators: Vec<Participant>,
}

#[derive(Debug, Clone)]
pub(crate) enum GameState {
    Initializing,
    New(Vec<Slot>),
    Ready(ReadyState),
}

#[derive(Clone)]
pub(crate) struct P2PGame {
    owner_id: PeerId,
    node: Node,
}

impl P2PGame {
    fn new(owner_id: &PeerId, node: &Node) -> Self {
        Self {
            owner_id: *owner_id,
            node: node.clone(),
        }
    }

    async fn get_slot_count(&self) -> Option<u8> {
        self.get_record("slot-count")
            .await
            .map(|slot_count_data| bincode::deserialize(&slot_count_data).unwrap())
    }

    async fn get_name(&self) -> Option<String> {
        self.get_record("name")
            .await
            .map(|name_data| bincode::deserialize(&name_data).unwrap())
    }

    pub(crate) async fn current_state(&self, p2p: &mut P2P) -> GameState {
        if let Some(slots) = self.get_slots(p2p).await {
            let participants = slots
                .iter()
                .filter_map(|slot| {
                    if let Slot::Occupied(occupied_slot) = slot {
                        Some(occupied_slot.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            let all_connected = participants.iter().all(|participant| {
                if let Participant::Remote(peer, _) = participant {
                    matches!(&*peer.connection_state.borrow(), PeerState::Connected(_))
                } else {
                    true // Local player is always ready!
                }
            });

            if participants.len() == slots.len() && all_connected {
                GameState::Ready(ReadyState {
                    players: participants,
                    spectators: vec![],
                })
            } else {
                GameState::New(slots)
            }
        } else {
            GameState::Initializing
        }
    }

    async fn get_slots(&self, p2p: &mut P2P) -> Option<Vec<Slot>> {
        if let Some(slot_count) = self.get_slot_count().await {
            let mut slots = Vec::with_capacity(slot_count as usize);
            for idx in 0..slot_count {
                let slot = match self.get_slot_owner(idx).await {
                    Some(slot_owner) => {
                        if slot_owner == self.node.local_peer_id {
                            Slot::Occupied(Participant::Local(slot_owner))
                        } else {
                            Slot::Occupied(Participant::Remote(
                                p2p.get_peer(slot_owner),
                                format!("127.0.0.1:{}", idx).parse().unwrap(),
                            ))
                        }
                    }
                    None => Slot::Vacant(),
                };
                slots.push(slot);
            }
            Some(slots)
        } else {
            None
        }
    }

    async fn get_slot_owner(&self, idx: u8) -> Option<PeerId> {
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

    pub(crate) fn claim_slot(&self, slot_idx: usize) {
        let key = format!("{}.slot-idx", self.node.local_peer_id);
        //println!("Claim slot {:?}, {:?}", slot_idx, key);
        let c = self.clone();
        tokio::spawn(async move {
            c.start_providing("slot-idx").await;
            c.put_record(&key, bincode::serialize(&slot_idx).unwrap())
                .await;
        });
    }

    async fn start_providing(&self, key: &str) {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.start_providing(&key).await;
    }

    async fn get_providers(&self, key: &str) -> HashSet<PeerId> {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.get_providers(&key).await
    }

    async fn put_record(&self, key: &str, value: Vec<u8>) {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.put_record(&key, value, None).await;
    }

    async fn get_record(&self, key: &str) -> Option<Vec<u8>> {
        let key = format!("{}.p2p-game.{}", self.owner_id, key);
        self.node.get_record(&key).await
    }
}

#[derive(Debug, Clone)]
pub(crate) struct P2P {
    node: Node,
    pub(crate) input_size: usize,
    peers: Arc<Mutex<HashMap<PeerId, Peer>>>,
}

impl P2P {
    pub(crate) async fn new(input_size: usize) -> Self {
        let node = discovery::Node::new().await;
        Self {
            node,
            input_size,
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn get_peer(&mut self, peer_id: PeerId) -> Peer {
        self.peers
            .lock()
            .unwrap()
            .entry(peer_id)
            .or_insert_with(|| Peer::new(peer_id, &self.node))
            .clone()
    }

    pub(crate) async fn find_games(&self, game_name: &str) -> Vec<(String, PeerId)> {
        let mut owners = vec![];
        let providers = self.node.get_providers("p2p-game").await;
        for peer_id in providers {
            for name in self
                .node
                .get_record(&format!("{}.p2p-game.name", peer_id))
                .await
            {
                let name: &str = bincode::deserialize(&name).unwrap();
                if name.contains(game_name) {
                    owners.push((name.to_owned(), peer_id));
                }
            }
        }
        owners
    }

    pub(crate) fn join_game(&self, owner_id: &PeerId) -> P2PGame {
        P2PGame::new(owner_id, &self.node)
    }

    pub(crate) fn create_game(&self, name: &str, slot_count: u8) -> P2PGame {
        let game = P2PGame::new(&self.node.local_peer_id, &self.node);
        tokio::spawn({
            let game = game.clone();
            let node = self.node.clone();
            let name = name.to_owned();
            async move {
                game.put_record("slot-count", bincode::serialize(&slot_count).unwrap())
                    .await;
                game.put_record("name", bincode::serialize(&name).unwrap())
                    .await;
                node.start_providing("p2p-game").await;
            }
        });

        game
    }

    pub(crate) fn start_session(&self, ready_state: &ReadyState) -> (P2PSession, usize) {
        let num_players = ready_state.players.len() + ready_state.spectators.len();
        println!("Players: {}", num_players);

        let sock = P2PGameNonBlockingSocket::new(ready_state);
        let mut session = P2PSession::new_with_socket(num_players as u32, self.input_size, sock)
            .expect("Could not create a P2P Session");

        let local_handle = {
            let mut local_handle = 0;
            for (slot_idx, slot) in ready_state.players.iter().enumerate() {
                match slot {
                    Participant::Local(_) => {
                        println!("Add local player {}", slot_idx);
                        local_handle = session.add_player(PlayerType::Local, slot_idx).unwrap();
                    }
                    Participant::Remote(_, addr) => {
                        println!("Add remote player {:?}", addr);
                        session
                            .add_player(PlayerType::Remote(*addr), slot_idx)
                            .unwrap();
                    }
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
    sender: tokio::sync::mpsc::Sender<(std::net::SocketAddr, UdpMessage)>,
}

impl P2PGameNonBlockingSocket {
    pub(crate) fn new(ready_state: &ReadyState) -> Self {
        let runtime_handle = tokio::runtime::Handle::current();
        let participants = [&ready_state.players[..], &ready_state.spectators[..]].concat();

        let mut peers = HashMap::new();
        for participant in participants {
            match participant {
                Participant::Remote(peer, addr) => {
                    peers.insert(addr, (peer, [0; RECV_BUFFER_SIZE]));
                }
                _ => (),
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
                                    if let Ok(msg) = bincode::deserialize::<UdpMessage>(
                                        &buffer[0..number_of_bytes],
                                    ) {
                                        //println!("READ: {:?} - {:?}", msg, src_addr);
                                        tx.send((src_addr, msg)).await.unwrap();
                                    } else {
                                        eprintln!(
                                            "Failed to deserialize message, message discarded"
                                        );
                                    }
                                }
                                println!("Exited read loop for {:?}", src_addr);
                            }
                            //TODO: exit when the peer state is unrecoverable
                            _ => eprintln!(
                                "Peer {:?} not in a state where it can read. Will try again...",
                                src_addr
                            ),
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            });
        }

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
                        eprintln!(
                            "Peer {:?} was not in a state where it could write. Message discarded.",
                            addr
                        );
                    }
                }
                println!("Exited write loop");
            }
        });

        Self {
            runtime_handle,
            reader,
            sender,
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
