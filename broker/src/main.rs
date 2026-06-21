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
                if stream.is_reliable() { 
                    if broker.is_existing_service(&connection) { 
                        if broker.is_existing_lifeline(&connection, &stream) { 
                            if let Some(AnyMessage::PubSub(pubsub_msg)) = decode_msg(&data) {
                                match pubsub_msg.op {
                                    PubSubOp::Pub => {
                                        let pub_stream = pubsub_msg.stream.unwrap_or(stream);
                                        broker.create_topic(pubsub_msg.topic, connection, pub_stream);
                                    }
                                    PubSubOp::StopPub => {
                                        broker.suppress_topic(pubsub_msg.topic, connection);
                                    }
                                    PubSubOp::Sub => {
                                        if let Some(client_id) = pubsub_msg.target {
                                            broker.force_subscribe_client(client_id, pubsub_msg.topic);
                                        } else {
                                            let sub_stream = pubsub_msg.stream.unwrap_or(stream);
                                            broker.subscribe(pubsub_msg.topic, connection, sub_stream);
                                        }
                                    }
                                    PubSubOp::StopSub => {
                                        if let Some(client_id) = pubsub_msg.target {
                                            broker.force_unsubscribe_client(client_id, pubsub_msg.topic);
                                        } else {
                                            broker.unsubscribe(pubsub_msg.topic, connection);
                                        }
                                    }
                                    PubSubOp::End => {
                                        broker.remove_service(&connection);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        else { 
                            broker.publish(&connection, &stream, data);
                        }

                        continue;
                    }
                    let msg = String::from_utf8_lossy(&data);
                    let msg = msg.trim();   

                    if msg.starts_with("JOIN ") {
                        let client_id = msg.trim_start_matches("JOIN ").trim().parse::<u32>().ok();
                        new_player_handler(&mut broker, &connection, &stream, client_id);
                    }

                    continue;
                }
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
    client_id: Option<u32>,
) {
    broker.register_service(&connection, &stream);
    if let Some(id) = client_id {
        broker.register_client_id(id, *connection);
    }

    let response = format!("WELCOME t");
    let _ = broker.peer.send(&connection, &stream, Bytes::from(response));
}