use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::Arc,
    task::{Context, Poll}
};

use libp2p::{
    development_transport, identity, NetworkBehaviour, PeerId,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    kad::{
        QueryId, KademliaEvent, PutRecordOk, QueryResult, Quorum, Record,
        record::{Key, store::MemoryStore}
    },
    swarm::{SwarmBuilder, NetworkBehaviourEventProcess, SwarmEvent},
    
};
use futures::prelude::*;
use tokio::sync::{broadcast::Receiver, mpsc::Sender};
use crate::peer::Peer;

pub(crate) type CommandBus = Arc<Sender<Command>>;
pub(crate) type EventBus = tokio::sync::broadcast::Sender<Event>;

pub struct Room {
    name: String,
    peers: HashMap<PeerId, Arc<Peer>>
}
impl Room {
    async fn new(name: &str, node: &Node) -> Self {
        node.command_bus.send(Command::ProvideSignal(name.to_string())).await.unwrap();
        Self {
            name: name.to_string(),
            peers: HashMap::new()
        }
    }

    pub(crate) async fn get_peers(self: &mut Self, node: &mut Node) -> Vec<Arc<Peer>> {
        let peer_ids = node.get_peers(&self.name).await.unwrap();
        for peer_id in peer_ids {
            let peer = self.peers.entry(peer_id);
            match peer {
                std::collections::hash_map::Entry::Occupied(_) => {},
                std::collections::hash_map::Entry::Vacant(_) => {
                    self.peers.insert(peer_id, Arc::new(Peer::new(peer_id, node.local_peer_id, node, &self.name).await));
                },
            }
        }

        self.peers.values().cloned().collect()
    }
}

type Kademlia = libp2p::kad::Kademlia<MemoryStore>;

// We create a custom network behaviour that combines Kademlia and mDNS.
#[derive(NetworkBehaviour)]
#[behaviour(event_process = true)]
struct MyBehaviour {
    kademlia: Kademlia,
    mdns: Mdns,

    #[behaviour(ignore)]
    event_bus: EventBus,

    #[behaviour(ignore)]
    #[allow(dead_code)]
    t: Receiver<Event>, //TODO: keep the event bus alive somehow.

    #[behaviour(ignore)]
    pending_commands: HashMap<QueryId, Command>
}

impl MyBehaviour {
    fn new(local_peer_id: PeerId, mdns: Mdns) -> Self {
        // Create a Kademlia behaviour.
        let store = MemoryStore::new(local_peer_id);
        let kademlia = Kademlia::new(local_peer_id, store);

        let (event_bus, t) = tokio::sync::broadcast::channel(16);
        
        Self {
            kademlia,
            mdns,
            event_bus,
            t,
            pending_commands: HashMap::new()
        }
    }
}

impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    // Called when `mdns` produces an event.
    fn inject_event(&mut self, event: MdnsEvent) {
        if let MdnsEvent::Discovered(list) = event {
            for (peer_id, multiaddr) in list {
                self.kademlia.add_address(&peer_id, multiaddr);
            }
        }
    }
}
use std::str::from_utf8;
impl NetworkBehaviourEventProcess<KademliaEvent> for MyBehaviour {
    // Called when `kademlia` produces an event.
    fn inject_event(&mut self, message: KademliaEvent) {
        match message {
            KademliaEvent::OutboundQueryCompleted { result, id: query_id, .. } => match result {
                QueryResult::GetRecord(Ok(ok)) => {
                    for r in ok.records {
                        let record = &r.record;
                        let key = SignalKey::from_key(&record.key).unwrap();
                        let value = &record.value;
                        self.event_bus.send(Event::SignalReceived(key, value.to_vec())).unwrap();
                    }
                    self.pending_commands.remove_entry(&query_id).unwrap();
                }
                QueryResult::PutRecord(Ok(PutRecordOk { .. })) => {
                }
                QueryResult::GetProviders(Ok(ok)) => {
                    self.pending_commands.remove_entry(&query_id);
                    self.event_bus.send(Event::PeersInRoomReceived(ok.providers)).unwrap();
                }
                QueryResult::PutRecord(Err(err)) => {
                    let (_, cmd) = self.pending_commands.remove_entry(&query_id).unwrap();
                    self.event_bus.send(Event::CommandFailed(cmd, format!("{:?}", err))).unwrap();
                }
                QueryResult::GetRecord(Err(err)) => {
                    let (_, cmd) = self.pending_commands.remove_entry(&query_id).unwrap();
                    self.event_bus.send(Event::CommandFailed(cmd, format!("{:?}", err))).unwrap();
                }

                _ => {}
            },
            _ => {}
        }
    }
}

pub(crate) struct Node {
    //TODO: Make this command/event bus thing into an async api instead..
    pub(crate) command_bus: CommandBus,
    pub(crate) event_bus: EventBus,
    pub(crate) local_peer_id: PeerId
}

impl Node {
    pub(crate) async fn new() -> Self {
        let (command_bus, event_bus, local_peer_id) = Node::setup_discovery().await;

        Self {
            command_bus, event_bus, local_peer_id
        }
    }

    async fn setup_discovery() -> (CommandBus, EventBus, PeerId) {
        // Create a random key for ourselves.
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        println!("We are peer {:?}", local_peer_id);
        // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
        let transport = development_transport(local_key).await.unwrap();

        // Create a swarm to manage peers and events.
        let mut swarm = {
            
            let mdns = Mdns::new(MdnsConfig::default()).await.unwrap();
            let behaviour = MyBehaviour::new(local_peer_id, mdns);
            SwarmBuilder::new(transport, behaviour, local_peer_id)
            // We want the connection background tasks to be spawned
            // onto the tokio runtime.
            .executor(Box::new(|fut| {
                tokio::spawn(fut);
            }))
            .build()
        };
 
        // Listen on all interfaces and whatever port the OS assigns.
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap()).unwrap();
        let (command_bus, mut command_bus_rx) = tokio::sync::mpsc::channel::<Command>(10);
        let event_bus = swarm.behaviour().event_bus.clone();
        // Kick it off.
        tokio::spawn(future::poll_fn(move |cx: &mut Context<'_>| {
            loop {
                match command_bus_rx.poll_recv(cx) {
                    
                    Poll::Ready(Some(cmd)) => {
                        match cmd.clone() {
                            Command::ProvideSignal(room_name) => {
                                let key = Key::new(&format!("{}.{}.signal_data", PREFIX, room_name));
                                let query_id = swarm.behaviour_mut().kademlia.start_providing(key).unwrap();
                                swarm.behaviour_mut().pending_commands.insert(query_id, cmd);
                            },
                            Command::GetPeersInRoom(room_name) => {
                                let key = Key::new(&format!("{}.{}.signal_data", PREFIX, room_name));
                                swarm.behaviour_mut().kademlia.get_providers(key);
                            },
                            Command::PutSignal(key, value) => {
                                let record = Record {
                                    key: key.to_key(),
                                    value: value,
                                    publisher: Some(local_peer_id),
                                    expires: None,
                                };
                                let query_id = swarm.behaviour_mut().kademlia.put_record(record, Quorum::One).unwrap();
                                swarm.behaviour_mut().pending_commands.insert(query_id, cmd);

                            },
                            Command::GetSignal(key) => {
                                let query_id = swarm.behaviour_mut().kademlia.get_record(&key.to_key(), Quorum::One);
                                swarm.behaviour_mut().pending_commands.insert(query_id, cmd);
                            }
                        }
                    },
                    Poll::Ready(None) => return Poll::Ready(Ok::<(), String>(())),
                    Poll::Pending => break,
                }
            }
            
            loop {
                match swarm.poll_next_unpin(cx) {
                    Poll::Ready(Some(event)) => {
                        if let SwarmEvent::NewListenAddr { address, .. } = event {
                            println!("Listening on {:?}", address);
                        }
                    }
                    Poll::Ready(None) => return Poll::Ready(Ok::<(), String>(())),
                    Poll::Pending => break,
                }
            }
            Poll::Pending
        }));

        (Arc::new(command_bus), event_bus, local_peer_id)
    }

    async fn get_peers(self: &mut Self, room_name: &str) -> Result<HashSet<PeerId>, String> {
        let cmd = Command::GetPeersInRoom(room_name.to_string());
        self.command_bus.send(cmd.clone()).await.unwrap();
        
        let mut event_bus = self.event_bus.subscribe();
        loop {
            match event_bus.recv().await.unwrap() {
                Event::PeersInRoomReceived(providers) => {
                    //TODO: check if the event matches the command
                    return Ok(providers);
                },
                Event::CommandFailed(failed_cmd, error) => {
                    if cmd == failed_cmd {
                        return Err(format!("Command ({:?}) failed: {:?} ", cmd, error));
                    }
                },
                _ => {}
            }
        }
    }

    pub async fn enter_room(self: &mut Self, room_name: &String) -> Room {
        Room::new(room_name, self).await
    }
}

const PREFIX: &str = "nestest";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SignalKey {
    //pub(crate) room_name: String,
    pub(crate) from_peer: PeerId,
    pub(crate) to_peer: PeerId
}

impl SignalKey {
    fn from_key(key: &Key) -> Result<SignalKey, String> {
        let key_data = key.to_vec();
        let parts = from_utf8(&key_data).unwrap().split(".").map(|s| s.to_string()).collect::<Vec<_>>();

        if parts.len() != 3 {
            Result::Err(format!("Malformed key {:?}", key))
        } else {
            assert_eq!(PREFIX, parts[0]);
            let peer_a = PeerId::from_str(&parts[1].to_string()).unwrap();
            let peer_b = PeerId::from_str(&parts[2].to_string()).unwrap();
            
            Ok(SignalKey { from_peer: peer_a, to_peer: peer_b })
        }

    }
    fn to_key(self: &Self) -> Key {
        Key::new(&format!("{}.{}.{}", PREFIX, self.from_peer, self.to_peer))
    }

}
type SignalData = Vec<u8>;

#[derive(Clone, Debug)]
pub(crate) enum Event {
    SignalReceived(SignalKey, SignalData),
    PeersInRoomReceived(HashSet<PeerId>),
    CommandFailed(Command, String)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    GetPeersInRoom(String),
    ProvideSignal(String),
    PutSignal(SignalKey, SignalData),
    GetSignal(SignalKey),
}
