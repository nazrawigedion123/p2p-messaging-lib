use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    ErrorEvent, MessageEvent, RtcDataChannel, RtcDataChannelEvent, RtcIceCandidate,
    RtcIceCandidateInit, RtcPeerConnection, RtcSdpType, RtcSessionDescription,
    RtcSessionDescriptionInit, RtcIceConnectionState, RtcPeerConnectionState,
    RtcPeerConnectionIceEvent,
};
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen_futures::JsFuture;

struct SharedState {
    data_channel: Option<RtcDataChannel>,
    on_message_callback: Option<js_sys::Function>,
    on_ice_candidate_callback: Option<js_sys::Function>,
    on_connection_state_change_callback: Option<js_sys::Function>,
    on_ice_connection_state_change_callback: Option<js_sys::Function>,
}

#[wasm_bindgen]
pub struct P2PConnectionShared {
    peer_connection: RtcPeerConnection,
    state: Rc<RefCell<SharedState>>,
}

#[wasm_bindgen]
impl P2PConnectionShared {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<P2PConnectionShared, JsValue> {
        console_error_panic_hook::set_once();
        
        let mut rtc_config = web_sys::RtcConfiguration::new();
        let ice_servers = js_sys::Array::new();
        let stun_server = web_sys::RtcIceServer::new();
        let urls = js_sys::Array::new();
        urls.push(&JsValue::from_str("stun:stun.l.google.com:19302"));
        stun_server.set_urls(&urls);
        ice_servers.push(&stun_server);
        rtc_config.set_ice_servers(&ice_servers);

        let peer_connection = RtcPeerConnection::new_with_configuration(&rtc_config)?;
        
        let state = Rc::new(RefCell::new(SharedState {
            data_channel: None,
            on_message_callback: None,
            on_ice_candidate_callback: None,
            on_connection_state_change_callback: None,
            on_ice_connection_state_change_callback: None,
        }));

        let connection = P2PConnectionShared {
            peer_connection,
            state,
        };
        
        // Setup ondatachannel handler
        let state_clone = connection.state.clone();
        let on_datachannel = Closure::wrap(Box::new(move |ev: RtcDataChannelEvent| {
            web_sys::console::log_1(&"Data channel received!".into());
            let channel = ev.channel();
            
            let mut state = state_clone.borrow_mut();
            state.data_channel = Some(channel.clone());
            
            // Setup listeners on this new channel
            if let Some(cb) = &state.on_message_callback {
                let cb_clone = cb.clone();
                let on_message = Closure::wrap(Box::new(move |ev: MessageEvent| {
                    if let Some(data) = ev.data().as_string() {
                        let _ = cb_clone.call1(&JsValue::NULL, &JsValue::from_str(&data));
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                on_message.forget();
            }
            
             let on_open = Closure::wrap(Box::new(move || {
                web_sys::console::log_1(&"Data channel open (remote)!".into());
            }) as Box<dyn FnMut()>);
            channel.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            on_open.forget();

        }) as Box<dyn FnMut(RtcDataChannelEvent)>);
        
        connection.peer_connection.set_ondatachannel(Some(on_datachannel.as_ref().unchecked_ref()));
        on_datachannel.forget();

        // Setup onicecandidate handler
        let state_clone_ice = connection.state.clone();
        let on_ice_candidate = Closure::wrap(Box::new(move |ev: RtcPeerConnectionIceEvent| {
            if let Some(candidate) = ev.candidate() {
                let state = state_clone_ice.borrow();
                if let Some(cb) = &state.on_ice_candidate_callback {
                    let json_str = js_sys::JSON::stringify(&candidate).unwrap_or(JsValue::from_str("{}").into());
                    let _ = cb.call1(&JsValue::NULL, &json_str);
                }
            }
        }) as Box<dyn FnMut(RtcPeerConnectionIceEvent)>);
        connection.peer_connection.set_onicecandidate(Some(on_ice_candidate.as_ref().unchecked_ref()));
        on_ice_candidate.forget();

        // Setup connection state change handlers
        let state_clone_conn = connection.state.clone();
        let pc_clone_conn = connection.peer_connection.clone();
        let on_connection_state_change = Closure::wrap(Box::new(move || {
            let state_str = format!("{:?}", pc_clone_conn.connection_state());
            let state = state_clone_conn.borrow();
            if let Some(cb) = &state.on_connection_state_change_callback {
                let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&state_str));
            }
        }) as Box<dyn FnMut()>);
        connection.peer_connection.set_onconnectionstatechange(Some(on_connection_state_change.as_ref().unchecked_ref()));
        on_connection_state_change.forget();

        let state_clone_ice_conn = connection.state.clone();
        let pc_clone_ice_conn = connection.peer_connection.clone();
        let on_ice_connection_state_change = Closure::wrap(Box::new(move || {
            let state_str = format!("{:?}", pc_clone_ice_conn.ice_connection_state());
            let state = state_clone_ice_conn.borrow();
            if let Some(cb) = &state.on_ice_connection_state_change_callback {
                let _ = cb.call1(&JsValue::NULL, &JsValue::from_str(&state_str));
            }
        }) as Box<dyn FnMut()>);
        connection.peer_connection.set_oniceconnectionstatechange(Some(on_ice_connection_state_change.as_ref().unchecked_ref()));
        on_ice_connection_state_change.forget();

        Ok(connection)
    }

    pub async fn create_offer(&self) -> Result<String, JsValue> {
        let data_channel = self.peer_connection.create_data_channel("chat");
        
        // Setup local data channel
        {
            let mut state = self.state.borrow_mut();
            state.data_channel = Some(data_channel.clone());
            
            let on_open = Closure::wrap(Box::new(move || {
                web_sys::console::log_1(&"Data channel open (local)!".into());
            }) as Box<dyn FnMut()>);
            data_channel.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            on_open.forget();
            
            // If we already have a message callback, attach it
            if let Some(cb) = &state.on_message_callback {
                 let cb_clone = cb.clone();
                 let on_message = Closure::wrap(Box::new(move |ev: MessageEvent| {
                    if let Some(data) = ev.data().as_string() {
                        let _ = cb_clone.call1(&JsValue::NULL, &JsValue::from_str(&data));
                    }
                }) as Box<dyn FnMut(MessageEvent)>);
                data_channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                on_message.forget();
            }
        }

        let offer_promise = self.peer_connection.create_offer();
        let offer = JsFuture::from(offer_promise).await?;
        let offer_sdp = offer.unchecked_into::<RtcSessionDescriptionInit>();
        
        let set_local_promise = self.peer_connection.set_local_description(&offer_sdp);
        JsFuture::from(set_local_promise).await?;
        
        let local_desc = self.peer_connection.local_description().ok_or("No local description")?;
        Ok(local_desc.sdp())
    }

    pub async fn create_answer(&self, offer_sdp: String) -> Result<String, JsValue> {
        let mut remote_desc_init = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
        remote_desc_init.set_sdp(&offer_sdp);
        
        let set_remote_promise = self.peer_connection.set_remote_description(&remote_desc_init);
        JsFuture::from(set_remote_promise).await?;

        let answer_promise = self.peer_connection.create_answer();
        let answer = JsFuture::from(answer_promise).await?;
        let answer_sdp = answer.unchecked_into::<RtcSessionDescriptionInit>();
        
        let set_local_promise = self.peer_connection.set_local_description(&answer_sdp);
        JsFuture::from(set_local_promise).await?;

        let local_desc = self.peer_connection.local_description().ok_or("No local description")?;
        Ok(local_desc.sdp())
    }

    pub async fn receive_answer(&self, answer_sdp: String) -> Result<(), JsValue> {
        let mut remote_desc_init = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
        remote_desc_init.set_sdp(&answer_sdp);
        
        let set_remote_promise = self.peer_connection.set_remote_description(&remote_desc_init);
        JsFuture::from(set_remote_promise).await?;
        Ok(())
    }

    pub async fn add_ice_candidate(&self, candidate_json: String) -> Result<(), JsValue> {
        let candidate_init = js_sys::JSON::parse(&candidate_json)?.unchecked_into::<RtcIceCandidateInit>();
        let promise = self.peer_connection.add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&candidate_init));
        JsFuture::from(promise).await?;
        Ok(())
    }

    pub fn close(&self) {
        let mut state = self.state.borrow_mut();
        if let Some(channel) = &state.data_channel {
            channel.close();
        }
        state.data_channel = None;
        self.peer_connection.close();
    }

    pub fn set_on_ice_candidate(&self, callback: js_sys::Function) {
        let mut state = self.state.borrow_mut();
        state.on_ice_candidate_callback = Some(callback);
    }

    pub fn set_on_connection_state_change(&self, callback: js_sys::Function) {
        let mut state = self.state.borrow_mut();
        state.on_connection_state_change_callback = Some(callback);
    }

    pub fn set_on_ice_connection_state_change(&self, callback: js_sys::Function) {
        let mut state = self.state.borrow_mut();
        state.on_ice_connection_state_change_callback = Some(callback);
    }

    pub fn set_on_message(&self, callback: js_sys::Function) {
        let mut state = self.state.borrow_mut();
        state.on_message_callback = Some(callback.clone());
        
        // If channel exists, attach now
        if let Some(channel) = &state.data_channel {
            let cb_clone = callback.clone();
            let on_message = Closure::wrap(Box::new(move |ev: MessageEvent| {
                if let Some(data) = ev.data().as_string() {
                    let _ = cb_clone.call1(&JsValue::NULL, &JsValue::from_str(&data));
                }
            }) as Box<dyn FnMut(MessageEvent)>);
            channel.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();
        }
    }

    pub fn send_message(&self, message: &str) -> Result<(), JsValue> {
        let state = self.state.borrow();
        if let Some(channel) = &state.data_channel {
            match channel.ready_state() {
                web_sys::RtcDataChannelState::Open => {
                    channel.send_with_str(message)?;
                    Ok(())
                }
                state => Err(JsValue::from_str(&format!(
                    "Data channel not open. Current state: {:?}",
                    state
                ))),
            }
        } else {
            Err(JsValue::from_str("Data channel not established"))
        }
    }
}

#[wasm_bindgen]
pub struct PeerManager {
    peers: Rc<RefCell<HashMap<String, Rc<P2PConnectionShared>>>>,
    on_peer_message: Rc<RefCell<Option<js_sys::Function>>>,
    on_peer_ice_candidate: Rc<RefCell<Option<js_sys::Function>>>,
    on_peer_connection_state_change: Rc<RefCell<Option<js_sys::Function>>>,
}

#[wasm_bindgen]
impl PeerManager {
    #[wasm_bindgen(constructor)]
    pub fn new() -> PeerManager {
        console_error_panic_hook::set_once();
        PeerManager {
            peers: Rc::new(RefCell::new(HashMap::new())),
            on_peer_message: Rc::new(RefCell::new(None)),
            on_peer_ice_candidate: Rc::new(RefCell::new(None)),
            on_peer_connection_state_change: Rc::new(RefCell::new(None)),
        }
    }

    pub fn add_peer(&self, peer_id: String) -> Result<(), JsValue> {
        let connection = Rc::new(P2PConnectionShared::new()?);
        
        // Setup callbacks for this peer to route back to manager
        
        // Message Callback
        let on_message_cb = self.on_peer_message.clone();
        let pid = peer_id.clone();
        let cb = Closure::wrap(Box::new(move |msg: String| {
            let cb_opt = on_message_cb.borrow();
            if let Some(func) = cb_opt.as_ref() {
                let _ = func.call2(&JsValue::NULL, &JsValue::from_str(&pid), &JsValue::from_str(&msg));
            }
        }) as Box<dyn FnMut(String)>);
        // We need to adapt the signature because P2PConnectionShared expects a function that takes a String (from the inner closure)
        // Wait, P2PConnectionShared::set_on_message takes a js_sys::Function.
        // The inner closure in P2PConnectionShared calls it with (JsValue::NULL, JsValue::from_str(&data)).
        // So we need a js_sys::Function that accepts (data).
        
        // Let's create the JS function wrapper
        let on_message_js_func: js_sys::Function = cb.into_js_value().unchecked_into();
        connection.set_on_message(on_message_js_func);


        // ICE Candidate Callback
        let on_ice_cb = self.on_peer_ice_candidate.clone();
        let pid_ice = peer_id.clone();
        let cb_ice = Closure::wrap(Box::new(move |candidate_json: String| {
            let cb_opt = on_ice_cb.borrow();
            if let Some(func) = cb_opt.as_ref() {
                let _ = func.call2(&JsValue::NULL, &JsValue::from_str(&pid_ice), &JsValue::from_str(&candidate_json));
            }
        }) as Box<dyn FnMut(String)>);
        let on_ice_js_func: js_sys::Function = cb_ice.into_js_value().unchecked_into();
        connection.set_on_ice_candidate(on_ice_js_func);


        // Connection State Callback
        let on_conn_cb = self.on_peer_connection_state_change.clone();
        let pid_conn = peer_id.clone();
        let cb_conn = Closure::wrap(Box::new(move |state: String| {
            let cb_opt = on_conn_cb.borrow();
            if let Some(func) = cb_opt.as_ref() {
                let _ = func.call2(&JsValue::NULL, &JsValue::from_str(&pid_conn), &JsValue::from_str(&state));
            }
        }) as Box<dyn FnMut(String)>);
        let on_conn_js_func: js_sys::Function = cb_conn.into_js_value().unchecked_into();
        connection.set_on_connection_state_change(on_conn_js_func);

        self.peers.borrow_mut().insert(peer_id, connection);
        Ok(())
    }

    pub fn remove_peer(&self, peer_id: &str) {
        if let Some(conn) = self.peers.borrow_mut().remove(peer_id) {
            conn.close();
        }
    }

    pub fn has_peer(&self, peer_id: &str) -> bool {
        self.peers.borrow().contains_key(peer_id)
    }

    pub async fn create_offer(&self, peer_id: String) -> Result<String, JsValue> {
        let peers = self.peers.borrow();
        if let Some(conn) = peers.get(&peer_id) {
            conn.create_offer().await
        } else {
            Err(JsValue::from_str("Peer not found"))
        }
    }

    pub async fn create_answer(&self, peer_id: String, offer_sdp: String) -> Result<String, JsValue> {
        let peers = self.peers.borrow();
        if let Some(conn) = peers.get(&peer_id) {
            conn.create_answer(offer_sdp).await
        } else {
            Err(JsValue::from_str("Peer not found"))
        }
    }

    pub async fn receive_answer(&self, peer_id: String, answer_sdp: String) -> Result<(), JsValue> {
        let peers = self.peers.borrow();
        if let Some(conn) = peers.get(&peer_id) {
            conn.receive_answer(answer_sdp).await
        } else {
            Err(JsValue::from_str("Peer not found"))
        }
    }

    pub async fn add_ice_candidate(&self, peer_id: String, candidate_json: String) -> Result<(), JsValue> {
        let peers = self.peers.borrow();
        if let Some(conn) = peers.get(&peer_id) {
            conn.add_ice_candidate(candidate_json).await
        } else {
            Err(JsValue::from_str("Peer not found"))
        }
    }

    pub fn send_message(&self, peer_id: String, message: String) -> Result<(), JsValue> {
        let peers = self.peers.borrow();
        if let Some(conn) = peers.get(&peer_id) {
            conn.send_message(&message)
        } else {
            Err(JsValue::from_str("Peer not found"))
        }
    }

    pub fn broadcast_message(&self, message: String) -> Result<(), JsValue> {
        let peers = self.peers.borrow();
        for (_, conn) in peers.iter() {
            // Ignore errors for individual peers, try to send to all
            let _ = conn.send_message(&message);
        }
        Ok(())
    }

    pub fn set_on_peer_message(&self, callback: js_sys::Function) {
        *self.on_peer_message.borrow_mut() = Some(callback);
    }

    pub fn set_on_peer_ice_candidate(&self, callback: js_sys::Function) {
        *self.on_peer_ice_candidate.borrow_mut() = Some(callback);
    }

    pub fn set_on_peer_connection_state_change(&self, callback: js_sys::Function) {
        *self.on_peer_connection_state_change.borrow_mut() = Some(callback);
    }
}