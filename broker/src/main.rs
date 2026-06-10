use shared::ServerInfo;

use game_sockets::protocols::UdpBackend;
use game_sockets::{GamePeer, GameNetworkEvent, /*GameConnection, GameStream, GameStreamReliability*/};

use bytes::Bytes;

mod pubsub;
use pubsub::broker::*;

fn main() {
    let broker_config = ServerInfo {
        ip: "127.0.0.1".to_string(),
        port: 5000,
        zone: "N/A".to_string(),
    };

    let mut peer = GamePeer::new(UdpBackend::new());
    peer.listen(&broker_config.ip, broker_config.port).expect("Failed to bind GamePeer");
    println!("🚀 Broker Server listening on {}:{}", broker_config.ip, broker_config.port);

    let mut broker = Broker::new(peer);

    loop {
        let Ok(Some(event)) = broker.peer.poll() else { continue } ;
        match event {
            GameNetworkEvent::Connected(conn) => {
                println!("New connection established: {:?}", conn.connection_id);
            }
            GameNetworkEvent::Disconnected(conn) => {
                println!("Connection lost: {:?}", conn.connection_id);
                broker.remove_service(conn);
            }
            GameNetworkEvent::Message { connection, stream, data } => {
                let msg = String::from_utf8_lossy(&data);
                let msg = msg.trim();

                if msg.starts_with("JOIN ") {
                    broker.register_service(connection, stream.clone());
                    
                    let response = format!("WELCOME t");
                    let _ = broker.peer.send(&connection, &stream, Bytes::from(response));
                    /*
                    let username = msg.replace("JOIN ", "").trim().to_string();
                    
                    if registry.players.contains_key(&connection) {
                        //println!("Player '{}' is already connected", username);
                        continue;
                    }

                    registry.players.insert(connection, PlayerInfo { username: username.clone() });

                    println!("Player '{}' joined the game!", username);

                    update_status(&config, &hb_res, &registry);

                    let response = format!("WELCOME {}", connection.connection_id);
                    let _ = network.peer.send(&connection, &stream, Bytes::from(response));*/
                }
            }
            GameNetworkEvent::StreamCreated(_connection, stream) => {
                eprintln!("Stream created : {:?}", stream);
            }
            GameNetworkEvent::Error { connection, inner } => {
                eprintln!("Error on connection {:?}: {}", connection.connection_id, inner);
            }
            _ => {} 
        }
    }
}
