use bevy::prelude::*;
use bytes::Bytes;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GameConnection, GameNetworkEvent, GamePeer, GameStream, GameStreamReliability};
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use uuid::Uuid;

#[derive(Resource)]
pub struct ServerConfig {
    pub id: String,
    pub shard_id: u32, 
    pub broker_addr: SocketAddr,
}

impl ServerConfig {
    fn from_env() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            shard_id: env::var("SHARD_ID").unwrap_or_else(|_| "0".to_string()).parse().unwrap(),
            broker_addr: env::var("BROKER_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:7002".to_string())
                .parse()
                .expect("Invalid broker address"),
        }
    }
}

pub struct PlayerState {
    pub position: Vec2,
    pub state: String, 
}

#[derive(Resource, Default)]
pub struct PlayerRegistry {
    pub players: HashMap<u32, PlayerState>,
}

#[derive(Resource)]
pub struct BrokerConnection {
    pub peer: GamePeer,
    pub connection: Option<GameConnection>,
    pub stream: Option<GameStream>,
}

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .insert_resource(ServerConfig::from_env())
        .init_resource::<PlayerRegistry>()
        .add_systems(Startup, connect_to_broker)
        .add_systems(Update, (poll_broker, simulate_and_publish).chain())
        .run();
}

fn connect_to_broker(mut commands: Commands, config: Res<ServerConfig>) {
    let backend = UdpBackend::new();
    let peer = GamePeer::new(backend);

    let broker_ip = config.broker_addr.ip().to_string();
    let broker_port = config.broker_addr.port();

    match peer.connect(&broker_ip, broker_port) {
        Ok(_) => println!("Shard {} attempting connection to broker at {}:{}", config.shard_id, broker_ip, broker_port),
        Err(e) => eprintln!("Failed to connect to broker: {:?}", e),
    }

    commands.insert_resource(BrokerConnection {
        peer,
        connection: None,
        stream: None,
    });
}

fn poll_broker(
    mut broker: ResMut<BrokerConnection>,
    mut registry: ResMut<PlayerRegistry>,
    config: Res<ServerConfig>,
) {
    while let Ok(Some(event)) = broker.peer.poll() {
        match event {
            GameNetworkEvent::Connected(conn) => {
                println!("Connected to Broker! Connection ID: {:?}", conn.connection_id);
                broker.connection = Some(conn);
                let _ = broker.peer.create_stream(conn, GameStreamReliability::Unreliable);
            }
            GameNetworkEvent::StreamCreated(_conn, stream) => {
                println!("Broker stream created.");
                broker.stream = Some(stream);
            }
            GameNetworkEvent::Disconnected(_) => {
                println!("Lost connection to Broker.");
                broker.connection = None;
                broker.stream = None;
            }
            GameNetworkEvent::Message { data, .. } => {
                if data.is_empty() { continue; }
                let tag = data[0];

                match tag {
                    0x05 => {
                        if data.len() >= 5 {
                            let client_id = u32::from_le_bytes(data[1..5].try_into().unwrap());
                            
                            registry.players.entry(client_id).or_insert(PlayerState {
                                position: Vec2::ZERO,
                                state: "Owned".to_string(),
                            });

                            println!("Received input from client: {}", client_id);
                        }
                    }
                    _ => {
                        println!("Received unknown tag from broker: {}", tag);
                    }
                }
            }
            GameNetworkEvent::Error { inner, .. } => {
                eprintln!("Broker connection error: {:?}", inner);
            }
            _ => {}
        }
    }
}

fn simulate_and_publish(
    broker: Res<BrokerConnection>,
    registry: Res<PlayerRegistry>,
    config: Res<ServerConfig>,
) {
    if let (Some(conn), Some(stream)) = (&broker.connection, &broker.stream) {
        
        for (&client_id, player_state) in registry.players.iter() {
            let mut payload: Vec<u8> = Vec::new();
            payload.push(0x10);
            payload.extend_from_slice(&client_id.to_le_bytes());
            payload.extend_from_slice(&player_state.position.x.to_le_bytes());
            payload.extend_from_slice(&player_state.position.y.to_le_bytes());

            let mut packet: Vec<u8> = Vec::new();
            packet.push(0x03);
            
            let topic_str = format!("shard:{}", config.shard_id);
            let mut topic_bytes = [0u8; 32];
            let tb = topic_str.as_bytes();
            let copy_len = std::cmp::min(tb.len(), 32);
            topic_bytes[..copy_len].copy_from_slice(&tb[..copy_len]);
            
            packet.extend_from_slice(&topic_bytes);
            
            let payload_len = payload.len() as u16;
            packet.extend_from_slice(&payload_len.to_le_bytes());
            packet.extend_from_slice(&payload);

            let _ = broker.peer.send(conn, stream, Bytes::from(packet));
        }
    }
}