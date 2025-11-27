use crate::model::{PeerInfo, SignalMessage};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use warp::ws::{Message, WebSocket};
use uuid::Uuid;
use std::sync::Arc;
use dashmap::DashMap;

// Type aliases for cleaner code
pub type Peers = Arc<DashMap<String, PeerState>>;

#[derive(Debug)]
pub struct PeerState {
    pub tx: mpsc::UnboundedSender<Result<Message, warp::Error>>,
    pub username: Option<String>,
    pub room: Option<String>,
}
pub async fn handle_connection(ws: WebSocket, peers: Peers) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    let client_rcv = tokio_stream::wrappers::UnboundedReceiverStream::new(client_rcv);
    let send_task = tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e);
        }
    }));

    let peer_id = Uuid::new_v4().to_string();

    println!("{} connected, awaiting login", peer_id);

    let initial_msg = client_ws_rcv.next().await;
    let mut username: Option<String> = None;

    if let Some(Ok(msg)) = initial_msg {
        if msg.is_text() {
            if let Ok(text) = msg.to_str() {
                if let Ok(signal_msg) = serde_json::from_str::<SignalMessage>(text) {
                    if let SignalMessage::Login { username: login_username } = signal_msg {
                        username = Some(login_username);
                    } else {
                        eprintln!("{} first message was not a login, disconnecting.", peer_id);
                    }
                } else {
                    eprintln!("Failed to parse initial message from {}: {}", peer_id, text);
                }
            }
        } else if msg.is_close() {
            println!("{} disconnected during initial handshake", peer_id);
        }
    } else {
        eprintln!("{} disconnected before sending initial message.", peer_id);
    }

    if let Some(user) = username {
        peers.insert(peer_id.clone(), PeerState {
            tx: client_sender,
            username: Some(user.clone()),
            room: None,
        });
        println!("{} logged in as {}", peer_id, user);

        while let Some(result) = client_ws_rcv.next().await {
            let msg = match result {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!("error receiving ws message for id: {}): {}", peer_id, e);
                    break;
                }
            };
            
            if msg.is_text() {
                if let Ok(text) = msg.to_str() {
                    if let Ok(signal_msg) = serde_json::from_str::<SignalMessage>(text) {
                        process_message(&peer_id, signal_msg, &peers).await;
                    } else {
                        eprintln!("Failed to parse message from {}: {}", peer_id, text);
                    }
                }
            } else if msg.is_close() {
                break;
            }
        }
    } else {
        // If login failed, ensure the send_task is aborted and connection is dropped.
        send_task.abort();
    }

    // Cleanup
    if let Some((_, state)) = peers.remove(&peer_id) {
        if let Some(room) = state.room {
            broadcast_peer_left(&room, &peer_id, &peers).await;
        }
    }
    println!("{} disconnected", peer_id);
}

async fn process_message(peer_id: &str, msg: SignalMessage, peers: &Peers) {
    match msg {
        SignalMessage::Login { username } => {
            if let Some(mut peer) = peers.get_mut(peer_id) {
                peer.username = Some(username);
                println!("{} logged in as {:?}", peer_id, peer.username);
            }
        }
        SignalMessage::Join { room } => {
            let mut old_room = None;
            let mut username = String::new();
            
            if let Some(mut peer) = peers.get_mut(peer_id) {
                old_room = peer.room.clone();
                peer.room = Some(room.clone());
                username = peer.username.clone().unwrap_or_else(|| "Anonymous".to_string());
            }

            if let Some(r) = old_room {
                broadcast_peer_left(&r, peer_id, peers).await;
            }

            // Notify others in the new room
            broadcast_peer_joined(&room, peer_id, &username, peers).await;

            // Send existing peers in the room to the joining peer
            let existing_peers = peers.iter()
                .filter(|p| p.key() != peer_id && p.room.as_deref() == Some(&room))
                .map(|p| PeerInfo {
                    peer_id: p.key().clone(),
                    username: p.username.clone().unwrap_or_else(|| "Anonymous".to_string()),
                })
                .collect();
            
            send_message(peer_id, SignalMessage::ExistingPeers { peers: existing_peers }, peers).await;
        }
        SignalMessage::Signal { target, data } => {
            // Forward the signal to the target peer
            // We wrap it in a Signal message again, but maybe we should have a wrapper?
            // The client expects "Signal" with target (sender) and data.
            // Wait, if A sends Signal to B, B needs to know it came from A.
            // So we should modify the message or send a new one.
            // Let's send a Signal message where 'target' is actually the 'source'.
            // Or better, change the definition of SignalMessage to have 'source' field?
            // But the client sends 'target'.
            // Let's just reuse Signal but interpret 'target' as 'sender' when receiving?
            // Or better: Server forwards it as:
            // Signal { target: SENDER_ID, data }
            // So the receiver knows who sent it.
            
            send_message(&target, SignalMessage::Signal { target: peer_id.to_string(), data }, peers).await;
        }
        _ => {}
    }
}

async fn broadcast_peer_joined(room: &str, peer_id: &str, username: &str, peers: &Peers) {
    let msg = SignalMessage::PeerJoined {
        peer_id: peer_id.to_string(),
        username: username.to_string(),
    };
    broadcast_to_room(room, msg, peer_id, peers).await;
}

async fn broadcast_peer_left(room: &str, peer_id: &str, peers: &Peers) {
    let msg = SignalMessage::PeerLeft {
        peer_id: peer_id.to_string(),
    };
    broadcast_to_room(room, msg, peer_id, peers).await;
}

async fn broadcast_to_room(room: &str, msg: SignalMessage, exclude_peer_id: &str, peers: &Peers) {
    let json = serde_json::to_string(&msg).unwrap();
    for peer in peers.iter() {
        if peer.key() != exclude_peer_id && peer.room.as_deref() == Some(room) {
             let _ = peer.tx.send(Ok(Message::text(&json)));
        }
    }
}

async fn send_message(peer_id: &str, msg: SignalMessage, peers: &Peers) {
    if let Some(peer) = peers.get(peer_id) {
        let json = serde_json::to_string(&msg).unwrap();
        let _ = peer.tx.send(Ok(Message::text(json)));
    }
}
