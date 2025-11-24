use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    ErrorEvent, MessageEvent, RtcDataChannel, RtcDataChannelEvent, RtcIceCandidate,
    RtcIceCandidateInit, RtcPeerConnection, RtcSdpType, RtcSessionDescription,
    RtcSessionDescriptionInit,
};
use std::rc::Rc;
use std::cell::RefCell;
use wasm_bindgen_futures::JsFuture;

struct SharedState {
    data_channel: Option<RtcDataChannel>,
    on_message_callback: Option<js_sys::Function>,
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
        }));

        let connection = P2PConnectionShared {
            peer_connection,
            state,
        };
        
        // Setup ondatachannel handler for the receiving side
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
        
        self.wait_for_ice_gathering().await?;
        
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

        self.wait_for_ice_gathering().await?;

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

    async fn wait_for_ice_gathering(&self) -> Result<(), JsValue> {
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let pc = self.peer_connection.clone();
            let resolve_clone = resolve.clone();
            
            if pc.ice_gathering_state() == web_sys::RtcIceGatheringState::Complete {
                let _ = resolve_clone.call0(&JsValue::NULL);
                return;
            }

            let pc_clone = pc.clone();
            let on_state_change = Closure::wrap(Box::new(move || {
                if pc_clone.ice_gathering_state() == web_sys::RtcIceGatheringState::Complete {
                    let _ = resolve_clone.call0(&JsValue::NULL);
                }
            }) as Box<dyn FnMut()>);

            pc.set_onicegatheringstatechange(Some(on_state_change.as_ref().unchecked_ref()));
            on_state_change.forget();
        });
        
        JsFuture::from(promise).await?;
        Ok(())
    }
}