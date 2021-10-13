use libp2p::kad::{AddProviderOk};
use libp2p::kad::record::Key;
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
pub(crate) enum OccupiedSlot {
    Local(PeerId),
    Remote(Peer, SocketAddr)
}
#[derive(Debug, Clone)]
enum Slot {
    Vacant,
    Occupied(OccupiedSlot)
}

pub(crate) struct P2PGame {
    name: String,
    peers: Arc<Mutex<HashMap<PeerId, Peer>>>,
    node: Node,
    num_players: usize,
}

const PREFIX: &str = "nestest";
impl P2PGame {
    async fn new(name: &str, num_players: usize, node: Node) -> Self {
        Self {
            name: name.to_string(),
            peers: Arc::new(Mutex::new(HashMap::new())),
            node,
            num_players,
        }
    }
    async fn get_peer(self: &Self, peer_id: PeerId) -> Peer {
        let mut lock = self.peers.lock().unwrap();
        if let Some(peer) = lock.get(&peer_id) {
            return peer.clone();
        } else {
            let peer = Peer::new(peer_id, self.node.clone()).await;
            lock.insert(peer_id, peer.clone());
            return peer;
        }
    }
    async fn get_slots(self: &Self) -> Vec<Slot> {
        let mut slots = Vec::with_capacity(self.num_players);
        for idx in 0..self.num_players {
            let slot = if let Some(slot_owners) = self.get_providers(&format!("slot-{}", idx)).await {
                // There might be many, pick the largest
                match slot_owners.iter().max() {
                    Some(&slot_owner) => {
                        if slot_owner == self.node.local_peer_id {
                            Slot::Occupied(OccupiedSlot::Local(slot_owner))
                        } else {
                            Slot::Occupied(OccupiedSlot::Remote(self.get_peer(slot_owner).await, format!("127.0.0.1:{}", idx).parse().unwrap()))
                        }
                    },
                    None => Slot::Vacant,
                }
            } else {
                Slot::Vacant
            };
            slots.push(slot);
        }
        slots
    }

    pub(crate) async fn start_session(self: &Self, input_size: usize) -> (P2PSession, usize) {
        let slots = self.get_slots().await;
        let occupied_slots = slots.iter()
        .filter_map(|slot| {
            if let Slot::Occupied(occupied_slot) = slot {
                Some(occupied_slot)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

        let num_players = occupied_slots.len();
        println!("Players: {}", num_players);
        
        let sock = P2PGameNonBlockingSocket::new(&occupied_slots).await;
        let mut session = P2PSession::new_with_socket(num_players as u32, input_size, sock).expect("Could not create a P2P Session");

        let local_handle = {
            let mut local_handle = 0;
            for (slot_idx, slot) in occupied_slots.iter().enumerate() {
                match slot {
                    OccupiedSlot::Local(_) => {
                        println!("Add local player {}", slot_idx);
                        local_handle = session.add_player(PlayerType::Local, slot_idx).unwrap();
                    },
                    OccupiedSlot::Remote(_, addr) => {
                        println!("Add remote player {:?}", addr);
                        session.add_player(PlayerType::Remote(addr.clone()), slot_idx).unwrap();
                    },
                }
            }
            local_handle
        };
        (session, local_handle)
    }

    pub(crate) async fn claim_slot(self: &Self, idx_to_claim: usize) {
        println!("Claim slot {:?}", idx_to_claim);

        for idx in 0..self.num_players {
            if idx != idx_to_claim {
                self.stop_providing(&format!("slot-{}", idx)).await;
            }
        }

        self.start_providing(&format!("slot-{}", idx_to_claim)).await;
    }

    /*
    async fn put_meta_data(self: &Self, key: &str, value: &str) -> PutRecordOk {
        let key = Key::new(&format!("{}.{}.{}", PREFIX, self.name, key));
        //println!("Putting meta data {:?}={:?}", key, value);
        let record = Record {
            key: key.clone(),
            value: value.as_bytes().to_vec(),
            publisher: Some(self.node.local_peer_id),
            expires: None,
        };
        loop {
            match self.node.put_record(record.clone()).await {
                Ok(ok) => break ok,
                Err(e) => {
                    eprintln!("Failed to put meta data with key '{:?}' ({}). Retrying...", key, e);
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
        
    }

    async fn get_meta_data(self: &Self, key: &str) -> Option<String> {
        let key = Key::new(&format!("{}.{}.{}", PREFIX, self.name, key));
        let key = key.clone();
        match self.node.get_record(key.clone()).await {
            Ok(ok) => {
                let mut result = None;
                //TODO: when getting many like this, which one to use?
                
                for record in ok.records {
                    //println!("Record: {:?}", record);
                    result = Some(record);
                }
                //println!("Using: {:?}", result);
    
                return result.map(|e| std::str::from_utf8(&e.record.value).unwrap().to_string() );
            },
            Err(_) => {
                //TODO: maybe check if it was some other error than NotFound?
                return None;
            }
        }
    }
    */
    async fn start_providing(self: &Self, key: &str) -> AddProviderOk {
        let key = Key::new(&format!("{}.{}.{}", PREFIX, self.name, key));
        loop {
            match self.node.start_providing(key.clone()).await {
                Ok(ok) => break ok,
                Err(e) => {
                    eprintln!("Failed to start providing key '{:?}' ({}). Retrying...", key, e);
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    }

    async fn stop_providing(self: &Self, key: &str) {
        let key = Key::new(&format!("{}.{}.{}", PREFIX, self.name, key));
        self.node.stop_providing(key.clone()).await;
    }

    async fn get_providers(self: &Self, key: &str) -> Option<HashSet<PeerId>> {
        let key = Key::new(&format!("{}.{}.{}", PREFIX, self.name, key));

        if let Ok(ok) = self.node.get_providers(key.clone()).await {
            Some(ok.providers)
        } else {
            //TODO: Return an error instead?
            eprintln!("Could not get providers for {:?}", key);
            None
        }
    }
}

pub(crate) struct P2P {
    node: Node,
}

impl P2P {
    pub(crate) async fn new() -> Self {
        let node = discovery::Node::new().await;
        Self { node }
    }

    pub(crate) async fn create_game(self: &Self, name: &str, num_players: usize) -> P2PGame {
        let game = P2PGame::new(name, num_players, self.node.clone()).await;

        loop {
            let slots = game.get_slots().await;    
            println!("Waiting for slots to be occupied and connected: {:?}", slots);

            let vacant_idx = slots.iter().enumerate()
            .find(|(_, slot) | matches!(slot, Slot::Vacant))
            .map(|(idx, _)| idx);

            if let Some(vacant_idx) = vacant_idx {
                let our_slot = slots.iter()
                .find(|slot| matches!(slot, Slot::Occupied(OccupiedSlot::Local(_))));
                if our_slot.is_none() {
                    game.claim_slot(vacant_idx).await;    
                }
            } else {
                // All slots occupied, are they all connected?
                let peers = slots.iter().filter_map(|slot| {
                    match slot {
                        Slot::Occupied(OccupiedSlot::Remote(peer, _)) => Some(peer),
                        _ => None,
                    }
                }).collect::<Vec<_>>();

                if peers.iter().all(|&peer| matches!(*peer.connection_state.borrow(), PeerState::Connected(_))) {
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
        
        println!("All slots occupied and connected!\n\n\n");
        game
    }
}

const RECV_BUFFER_SIZE: usize = 4096;
pub(crate) struct P2PGameNonBlockingSocket {
    runtime_handle: Handle,
    reader: tokio::sync::mpsc::Receiver<(std::net::SocketAddr, UdpMessage)>,
    sender: tokio::sync::mpsc::Sender<(std::net::SocketAddr, UdpMessage)>
}

impl P2PGameNonBlockingSocket {
    pub(crate) async fn new(slots: &Vec<&OccupiedSlot>) -> Self {
        let runtime_handle = tokio::runtime::Handle::current();
        
        let channels = slots.iter().filter_map(|slot| {
            match slot {
                OccupiedSlot::Remote(peer, addr) => {
                    if let PeerState::Connected(channel) = peer.connection_state.borrow().clone() {
                        Some((addr.clone(), (channel, [0; RECV_BUFFER_SIZE])))
                    } else {
                        None
                    }
                },
                _ => None
            }
        }).collect::<HashMap<_, _>>();

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
                            tx.send((src_addr.clone(), msg)).await.unwrap();
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
        f.debug_struct("RoomNonBlockingSocket").finish()
    }
}