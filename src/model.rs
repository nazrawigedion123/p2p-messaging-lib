use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum SignalMessage {
    Login { username: String },
    Join { room: String },
    Signal { target: String, data: serde_json::Value }, // data can be Offer, Answer, or IceCandidate
    // Server -> Client messages
    PeerJoined { peer_id: String, username: String },
    PeerLeft { peer_id: String },
    ExistingPeers { peers: Vec<PeerInfo> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub username: String,
}
