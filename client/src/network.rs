use shared::*;
use shared::messages::netmessage::{decode_msg, send_msg, AnyMessage, PubSubMessage, PubSubOp};
use shared::messages::topics::Topic;
use bevy::prelude::*;
use std::collections::HashMap;

use game_sockets::protocols::UdpBackend;
use game_sockets::{GamePeer, GameNetworkEvent, GameConnection, GameStream, GameStreamReliability};

use crate::AppState;

#[derive(Resource)]
pub struct GameServerInfo(pub ServerInfo);

#[derive(Resource, Default)]
pub struct LocalPlayer {
    pub id: u32,
}

#[derive(Resource, Default)]
pub struct WorldView {
    pub players: HashMap<u32, Vec2>,
    pub enemies: HashMap<u32, Vec2>,
    pub projectiles: HashMap<u32, Vec2>,
}

#[derive(Resource, Default)]
pub struct PlayerStats {
    pub hp: i32,
    pub score: u32,
    pub wave: u32,
}

#[derive(Resource)]
pub struct NetworkClient {
    pub peer: GamePeer,
    pub connection: Option<GameConnection>,
    pub reliable_stream: Option<GameStream>,
    pub unreliable_stream: Option<GameStream>,
    pub registered: bool,
}

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        let peer = GamePeer::new(UdpBackend::new());

        app.insert_resource(NetworkClient {
            peer,
            connection: None,
            reliable_stream: None,
            unreliable_stream: None,
            registered: false,
        })
        .init_resource::<LocalPlayer>()
        .init_resource::<WorldView>()
        .init_resource::<PlayerStats>()
        .add_systems(OnEnter(AppState::Connecting), setup_connection)
        .add_systems(Update, connection_request.run_if(in_state(AppState::Connecting)))
        .add_systems(Update, network_poll)
        .add_systems(Update, register_pubsub.run_if(in_state(AppState::InGame)));
    }
}

fn setup_connection(client: ResMut<NetworkClient>, game_server: Res<GameServerInfo>) {
    if let Err(e) = client.peer.connect(&game_server.0.ip, game_server.0.port) {
        error!("Connection failed: {:?}", e);
    } else {
        println!("Connecting to broker {}:{}...", game_server.0.ip, game_server.0.port);
    }
}

fn connection_request(client: ResMut<NetworkClient>, local: Res<LocalPlayer>) {
    if let (Some(connection), Some(stream)) = (&client.connection, &client.reliable_stream) {

        let join = format!("JOIN {}", local.id);
        let _ = client.peer.send(connection, stream, join.into());
    }
}


fn register_pubsub(mut client: ResMut<NetworkClient>) {
    if client.registered || client.unreliable_stream.is_none() {
        return;
    }

    let ok = if let (Some(connection), Some(reliable)) =
        (&client.connection, &client.reliable_stream)
    {
        let stream = client.unreliable_stream.clone();
        let pub_msg = PubSubMessage {
            op: PubSubOp::Pub,
            topic: Topic::Input(0),
            stream: stream.clone(),
            target: None,
        };
        let _ = send_msg(&client.peer, connection, reliable, &pub_msg);

        let sub_msg = PubSubMessage {
            op: PubSubOp::Sub,
            topic: Topic::Snapshot(0),
            stream,
            target: None,
        };
        let _ = send_msg(&client.peer, connection, reliable, &sub_msg);
        true
    } else {
        false
    };

    if ok {
        client.registered = true;
        println!("Registered with broker: publishing Input(0), subscribed to Snapshot(0).");
    }
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(f32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(i32::from_le_bytes(bytes.try_into().ok()?))
}

pub fn network_poll(
    mut client: ResMut<NetworkClient>,
    mut world: ResMut<WorldView>,
    mut stats: ResMut<PlayerStats>,
    local: Res<LocalPlayer>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    while let Ok(Some(event)) = client.peer.poll() {
        match event {
            GameNetworkEvent::Connected(connection) => {
                println!("Connected to server: {:?}", connection.connection_id);
                client.connection = Some(connection);
                let _ = client.peer.create_stream(connection, GameStreamReliability::Reliable);
                let _ = client.peer.create_stream(connection, GameStreamReliability::Unreliable);
            }
            GameNetworkEvent::Disconnected(_) => {
                println!("Disconnected from server.");
                client.connection = None;
                client.reliable_stream = None;
                client.unreliable_stream = None;
                client.registered = false;
                world.players.clear();
                world.enemies.clear();
                world.projectiles.clear();
                next_state.set(AppState::Disconnected);
            }
            GameNetworkEvent::StreamCreated(_connection, stream) => {
                if stream.is_reliable() {
                    client.reliable_stream = Some(stream);
                } else {
                    client.unreliable_stream = Some(stream);
                }
            }
            GameNetworkEvent::Message { stream, data, .. } => {
                if stream.is_reliable() {
                    if let Some(AnyMessage::PubSub(_)) = decode_msg(&data) {
                    } else {
                        let text = String::from_utf8_lossy(&data);
                        if text.trim_start().starts_with("WELCOME") {
                            println!("Welcomed by broker; entering game.");
                            next_state.set(AppState::InGame);
                        } else if text.trim_start().starts_with("REJECT") {
                            next_state.set(AppState::Rejected);
                        }
                    }
                } else if !data.is_empty() {
                    match data[0] {
                        0x10 => {
                            if let (Some(id), Some(x), Some(y)) =
                                (read_u32(&data, 1), read_f32(&data, 5), read_f32(&data, 9))
                            {
                                world.players.insert(id, Vec2::new(x, y));
                            }
                        }
                        0x40 => {
                            if let (Some(id), Some(x), Some(y)) =
                                (read_u32(&data, 1), read_f32(&data, 5), read_f32(&data, 9))
                            {
                                world.enemies.insert(id, Vec2::new(x, y));
                            }
                        }
                        0x41 => {
                            if let Some(id) = read_u32(&data, 1) {
                                world.enemies.remove(&id);
                            }
                        }
                        0x50 => {
                            if let (Some(id), Some(x), Some(y)) =
                                (read_u32(&data, 1), read_f32(&data, 5), read_f32(&data, 9))
                            {
                                world.projectiles.insert(id, Vec2::new(x, y));
                            }
                        }
                        0x51 => {
                            if let Some(id) = read_u32(&data, 1) {
                                world.projectiles.remove(&id);
                            }
                        }
                        0x11 => {
                            if let (Some(id), Some(hp), Some(score), Some(wave)) = (
                                read_u32(&data, 1),
                                read_i32(&data, 5),
                                read_u32(&data, 9),
                                read_u32(&data, 13),
                            ) {
                                if id == local.id {
                                    stats.hp = hp;
                                    stats.score = score;
                                    stats.wave = wave;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            GameNetworkEvent::Error { inner, .. } => {
                eprintln!("Error from server: {:?}", inner);
            }
            GameNetworkEvent::StreamClosed(_, _) => {}
        }
    }
}
