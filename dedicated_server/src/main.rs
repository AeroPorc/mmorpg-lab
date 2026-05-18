use bevy::prelude::*;
use bytes::Bytes;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GameConnection, GameNetworkEvent, GamePeer, GameStreamReliability};
use shared::Heartbeat;
use std::collections::HashMap;
use std::env;
use std::net::{SocketAddr, UdpSocket};
use uuid::Uuid;

#[derive(Resource)]
pub struct ServerConfig {
    pub id: String,
    pub port: u16,
    pub zone: String,
    pub max_players: usize,
    pub orchestrator_addr: SocketAddr,
}

impl ServerConfig {
    fn from_env() -> Self {
        let port_str = env::var("DS_PORT").unwrap_or_else(|_| "7001".to_string());
        let port: u16 = port_str.parse().expect("DS_PORT must be a valid number");

        Self {
            id: Uuid::new_v4().to_string(),
            port,
            zone: env::var("DS_ZONE").unwrap_or_else(|_| "zone_A".to_string()),
            max_players: 100,
            orchestrator_addr: "127.0.0.1:8000"
                .parse()
                .expect("Invalid orchestrator address"),
        }
    }
}

pub struct PlayerInfo {
    pub username: String,
}

#[derive(Resource, Default)]
pub struct PlayerRegistry {
    pub players: HashMap<GameConnection, PlayerInfo>,
}

#[derive(Resource)]
pub struct NetworkRes {
    pub peer: GamePeer,
}

#[derive(Resource)]
pub struct HeartbeatSocket {
    pub socket: UdpSocket,
}

fn main() {
    App::new()
        .add_plugins(MinimalPlugins)
        .insert_resource(ServerConfig::from_env())
        .init_resource::<PlayerRegistry>()
        .add_systems(Startup, bind_sockets)
        .add_systems(Update, (receive_packets, send_heartbeat).chain())
        .run();
}

fn bind_sockets(mut commands: Commands, config: Res<ServerConfig>) {
    let bind_addr = "0.0.0.0";
    
    let backend = UdpBackend::new();
    let peer = GamePeer::new(backend);
    
    peer.listen(bind_addr, config.port).expect("Failed to bind GamePeer");
    println!("🚀 Dedicated Server [{}] listening on {}:{}", config.id, bind_addr, config.port);
    commands.insert_resource(NetworkRes { peer });

    let hb_socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind heartbeat socket");
    commands.insert_resource(HeartbeatSocket { socket: hb_socket });
}

fn receive_packets(
    mut network: ResMut<NetworkRes>,
    mut registry: ResMut<PlayerRegistry>,
    config: Res<ServerConfig>,
) {
    while let Ok(Some(event)) = network.peer.poll() {
        match event {
            GameNetworkEvent::Connected(conn) => {
                println!("New connection established: {:?}", conn.connection_id);
                let _ = network.peer.create_stream(conn, GameStreamReliability::Reliable);
            }
            GameNetworkEvent::Disconnected(conn) => {
                println!("Connection lost: {:?}", conn.connection_id);
                registry.players.remove(&conn);
            }
            GameNetworkEvent::Message { connection, stream, data } => {
                let msg = String::from_utf8_lossy(&data);
                let msg = msg.trim();

                if msg.starts_with("JOIN") {
                    let username = msg.replace("JOIN ", "").trim().to_string();
                    
                    if registry.players.len() >= config.max_players {
                        let _ = network.peer.send(&connection, &stream, Bytes::from("REJECT Server Full"));
                        continue;
                    }

                    registry.players.insert(connection, PlayerInfo { username: username.clone() });
                    println!("Player '{}' joined the game!", username);

                    let response = format!("WELCOME {}", connection.connection_id);
                    let _ = network.peer.send(&connection, &stream, Bytes::from(response));
                }
            }
            GameNetworkEvent::Error { connection, inner } => {
                eprintln!("Error on connection {:?}: {}", connection.connection_id, inner);
            }
            _ => {} 
        }
    }
}

fn send_heartbeat(
    time: Res<Time>,
    mut timer: Local<f32>,
    config: Res<ServerConfig>,
    hb_res: Res<HeartbeatSocket>,
    registry: Res<PlayerRegistry>,
) {
    *timer += time.delta_secs();

    if *timer >= 5.0 {
        *timer = 0.0;

        let heartbeat = Heartbeat {
            id: config.id.clone(),
            ip: "127.0.0.1".to_string(), 
            port: config.port,
            zone: config.zone.clone(),
            player_count: registry.players.len(),
            max_players: config.max_players,
        };

        if let Ok(payload) = serde_json::to_string(&heartbeat) {
            let _ = hb_res.socket.send_to(payload.as_bytes(), config.orchestrator_addr);
            println!("Heartbeat sent (Players: {}/{})", heartbeat.player_count, heartbeat.max_players);
        }
    }
}