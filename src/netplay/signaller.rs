//! Matchbox's WebSocket signaller, plus a flag for when another player shows up.
//!
//! Matchbox reports a peer only once the WebRTC connection to it is up, so on its own there's no
//! telling "nobody has joined yet" from "someone joined, but we can't reach them". The second is
//! what happens without a TURN relay when a direct connection isn't possible, and it's when we
//! want to offer unlocking one. Matchbox's own signaller isn't public, so this is a copy of it
//! that also watches the signaling events go by.
//!
//! TODO: Fix this upstream and delete this module. Have matchbox report a peer whose handshake
//! has started (e.g. a `PeerState::Connecting`); with matchbox reporting a failed handshake as
//! `Disconnected` (johanhelsing/matchbox#580, merged after 0.15.0) that's all `peer_up` needs.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use async_tungstenite::{
    WebSocketStream,
    smol::{ConnectStream, connect_async},
    tungstenite::Message,
};
use futures::StreamExt;
use matchbox_socket::{
    PeerEvent, PeerRequest, SignalingError, Signaller, SignallerBuilder, async_trait::async_trait,
};

/// Whether another player is in the room with us, shared with the signaller.
#[derive(Debug, Default, Clone)]
pub struct OpponentFound(Arc<AtomicBool>);

impl OpponentFound {
    pub fn get(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    fn set(&self, found: bool) {
        self.0.store(found, Ordering::Release);
    }
}

#[derive(Debug)]
pub struct ObservingSignallerBuilder {
    opponent_found: OpponentFound,
}

impl ObservingSignallerBuilder {
    pub fn new(opponent_found: OpponentFound) -> Self {
        Self { opponent_found }
    }
}

#[async_trait]
impl SignallerBuilder for ObservingSignallerBuilder {
    async fn new_signaller(
        &self,
        mut attempts: Option<u16>,
        room_url: String,
    ) -> Result<Box<dyn Signaller>, SignalingError> {
        let websocket_stream = loop {
            match connect_async(&room_url).await {
                Ok((websocket_stream, _)) => break websocket_stream,
                Err(e) => match attempts.as_mut() {
                    Some(remaining) if *remaining <= 1 => {
                        return Err(SignalingError::NegotiationFailed(Box::new(
                            SignalingError::from(e),
                        )));
                    }
                    Some(remaining) => {
                        *remaining -= 1;
                        log::warn!(
                            "Connection to the signaling server failed ({e}), {remaining} attempt(s) left"
                        );
                        futures_timer::Delay::new(Duration::from_secs(3)).await;
                    }
                    None => futures_timer::Delay::new(Duration::from_secs(3)).await,
                },
            }
        };
        Ok(Box::new(ObservingSignaller {
            websocket_stream,
            opponent_found: self.opponent_found.clone(),
        }))
    }
}

struct ObservingSignaller {
    websocket_stream: WebSocketStream<ConnectStream>,
    opponent_found: OpponentFound,
}

#[async_trait]
impl Signaller for ObservingSignaller {
    async fn send(&mut self, request: PeerRequest) -> Result<(), SignalingError> {
        let request = serde_json::to_string(&request).expect("a serializable request");
        self.websocket_stream
            .send(Message::Text(request.into()))
            .await
            .map_err(SignalingError::from)
    }

    async fn next_message(&mut self) -> Result<PeerEvent, SignalingError> {
        let message = match self.websocket_stream.next().await {
            Some(Ok(Message::Text(message))) => message,
            Some(Ok(_)) => return Err(SignalingError::UnknownFormat),
            Some(Err(e)) => return Err(SignalingError::from(e)),
            None => return Err(SignalingError::StreamExhausted),
        };
        let event: PeerEvent = serde_json::from_str(&message).map_err(|e| {
            log::error!("Failed to deserialize signaling message: {e:?}");
            SignalingError::UnknownFormat
        })?;

        match &event {
            // The server tells the first player about the second one (NewPeer); the second one
            // only hears from the first one through its WebRTC offer (Signal).
            PeerEvent::NewPeer(_) | PeerEvent::Signal { .. } => self.opponent_found.set(true),
            PeerEvent::PeerLeft(_) => self.opponent_found.set(false),
            PeerEvent::IdAssigned(_) => {}
        }
        Ok(event)
    }
}
