use shared::ServerInfo;
use shared::messages::netmessage::{decode_msg, AnyMessage, PubSubOp};

use game_sockets::protocols::UdpBackend;
use game_sockets::{GamePeer, GameNetworkEvent, GameConnection, GameStream};

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
                broker.remove_service(&conn);
            }
            GameNetworkEvent::Message { connection, stream, data } => {
                if stream.is_reliable() { // special case for reliable streams (assert if the reliable stream is a lifeline)
                    if broker.is_existing_service(&connection) { // if has already a corresponding service
                        if broker.is_existing_lifeline(&connection, &stream) { // if given stream is the lifeline of the corresponding service -> handle the pubs / subs received
                            if let Some(AnyMessage::PubSub(pubsub_msg)) = decode_msg(&data) {
                                match pubsub_msg.op {
                                    PubSubOp::Pub => {
                                        broker.create_topic(pubsub_msg.topic, connection, stream);
                                    }
                                    PubSubOp::StopPub => {
                                        broker.suppress_topic(pubsub_msg.topic, connection);
                                    }
                                    PubSubOp::Sub => {
                                        broker.subscribe(pubsub_msg.topic, connection, stream);
                                    }
                                    PubSubOp::StopSub => {
                                        broker.unsubscribe(pubsub_msg.topic, connection);
                                    }
                                    PubSubOp::End => {
                                        broker.remove_service(&connection);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        else { // just some pubications to be transfered
                            broker.publish(&connection, &stream, data);
                        }

                        continue;
                    }

                    // Else is a new connection for a new service on a new lifeline
                    let msg = String::from_utf8_lossy(&data);
                    let msg = msg.trim();   

                    if msg.starts_with("JOIN ") { // New Player (JOIN is the "password for a new player connection")
                        new_player_handler(&mut broker, &connection, &stream);
                    }

                    continue;
                }
                // else is unreliable -> pure gameplay to be transfered
                broker.publish(&connection, &stream, data);
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

pub fn new_player_handler(
    broker: &mut Broker,
    connection: &GameConnection,
    stream: &GameStream,
) {
    broker.register_service(&connection, &stream); // register player with received connection (identifier of the player) and stream (lifeline of the newly created service)
                        
    let response = format!("WELCOME t");
    let _ = broker.peer.send(&connection, &stream, Bytes::from(response));
}