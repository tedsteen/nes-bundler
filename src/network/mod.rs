use futures::{select, FutureExt};
use futures_timer::Delay;
use ggrs::{Config, P2PSession};
use matchbox_socket::WebRtcSocket;
use std::time::Duration;

use crate::{MyGameState};

#[derive(Debug)]
pub(crate) struct GGRSConfig;
impl Config for GGRSConfig {
    type Input = u8;
    type State = MyGameState;
    type Address = String;
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum NetplayState {
    Disconnected,
    Connecting(Option<WebRtcSocket>),
    Connected(P2PSession<GGRSConfig>)
}

pub(crate) fn connect(room: &str) -> WebRtcSocket {
    println!("Connecting...");

    let (socket, loop_fut) = WebRtcSocket::new(format!("ws://matchbox.marati.s3n.io:3536/{}", room));

    println!("my id is {:?}", socket.id());

    let loop_fut = loop_fut.fuse();
    tokio::spawn(async move {
        futures::pin_mut!(loop_fut);

        let timeout = Delay::new(Duration::from_millis(100));
        futures::pin_mut!(timeout);
    
        loop {
            select! {
                _ = (&mut timeout).fuse() => {
                    timeout.reset(Duration::from_millis(100));
                }
    
                _ = &mut loop_fut => {
                    break;
                }
            }
        }    
    });

    socket
}