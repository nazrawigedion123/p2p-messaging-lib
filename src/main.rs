mod handler;
mod model;

use std::sync::Arc;
use dashmap::DashMap;
use warp::Filter;

#[tokio::main]
async fn main() {
    let peers = Arc::new(DashMap::new());
    let peers_filter = warp::any().map(move || peers.clone());

    let chat = warp::path("ws")
        .and(warp::ws())
        .and(peers_filter)
        .map(|ws: warp::ws::Ws, peers| {
            ws.on_upgrade(move |socket| handler::handle_connection(socket, peers))
        });

    println!("Signaling server started on 0.0.0.0:8000");
    warp::serve(chat).run(([0, 0, 0, 0], 8000)).await;
}
