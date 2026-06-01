use bevy::prelude::*;
use bytes::Bytes;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GameConnection, GameNetworkEvent, GamePeer, GameStream, GameStreamReliability};
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use uuid::Uuid;

const SHARD_WIDTH: f32 = 256.0;
const HANDOFF_MARGIN: f32 = 24.0;
const ENTITY_SPEED: f32 = 12.0;
const STATE_BYTES: usize = 64;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AuthorityState {
    Owned,
    PendingHandoff,
    Ghost,
}

#[derive(Clone, Debug)]
pub struct PlayerState {
    pub position: Vec2,
    pub velocity: Vec2,
    pub authority: AuthorityState,
    pub handoff_target: Option<u32>,
    pub handoff_ticks: u8,
    pub state_blob: [u8; STATE_BYTES],
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

fn topic_name(shard_id: u32) -> String {
    format!("shard:{}", shard_id)
}

fn topic_bytes(topic: &str) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    let raw = topic.as_bytes();
    let copy_len = std::cmp::min(raw.len(), bytes.len());
    bytes[..copy_len].copy_from_slice(&raw[..copy_len]);
    bytes
}

fn encode_publish(topic: &str, payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(1 + 32 + 2 + payload.len());
    packet.push(0x03);
    packet.extend_from_slice(&topic_bytes(topic));
    packet.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    packet.extend_from_slice(payload);
    packet
}

fn encode_subscribe(client_id: u32, topic: &str) -> Vec<u8> {
    let mut packet = Vec::with_capacity(1 + 4 + 32);
    packet.push(0x01);
    packet.extend_from_slice(&client_id.to_le_bytes());
    packet.extend_from_slice(&topic_bytes(topic));
    packet
}

fn build_state_blob(entity: &PlayerState) -> [u8; STATE_BYTES] {
    let mut blob = [0u8; STATE_BYTES];
    blob[0] = match entity.authority {
        AuthorityState::Owned => 0,
        AuthorityState::PendingHandoff => 1,
        AuthorityState::Ghost => 2,
    };

    if let Some(target) = entity.handoff_target {
        blob[1..5].copy_from_slice(&target.to_le_bytes());
    }

    blob[5..9].copy_from_slice(&entity.position.x.to_le_bytes());
    blob[9..13].copy_from_slice(&entity.position.y.to_le_bytes());
    blob[13..17].copy_from_slice(&entity.velocity.x.to_le_bytes());
    blob[17..21].copy_from_slice(&entity.velocity.y.to_le_bytes());
    blob
}

fn encode_handoff_request(entity_id: u32, entity: &PlayerState) -> Vec<u8> {
    let mut payload = Vec::with_capacity(1 + 4 + 16 + STATE_BYTES);
    payload.push(0x20);
    payload.extend_from_slice(&entity_id.to_le_bytes());
    payload.extend_from_slice(&entity.position.x.to_le_bytes());
    payload.extend_from_slice(&entity.position.y.to_le_bytes());
    payload.extend_from_slice(&entity.velocity.x.to_le_bytes());
    payload.extend_from_slice(&entity.velocity.y.to_le_bytes());
    payload.extend_from_slice(&entity.state_blob);
    payload
}

fn encode_handoff_ack(tag: u8, entity_id: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(1 + 4);
    payload.push(tag);
    payload.extend_from_slice(&entity_id.to_le_bytes());
    payload
}

fn encode_ghost_update(entity_id: u32, entity: &PlayerState) -> Vec<u8> {
    let mut payload = Vec::with_capacity(1 + 4 + 16);
    payload.push(0x23);
    payload.extend_from_slice(&entity_id.to_le_bytes());
    payload.extend_from_slice(&entity.position.x.to_le_bytes());
    payload.extend_from_slice(&entity.position.y.to_le_bytes());
    payload.extend_from_slice(&entity.velocity.x.to_le_bytes());
    payload.extend_from_slice(&entity.velocity.y.to_le_bytes());
    payload
}

fn encode_handoff_complete(entity_id: u32) -> Vec<u8> {
    encode_handoff_ack(0x24, entity_id)
}

fn shard_bounds(shard_id: u32) -> (f32, f32) {
    let left = shard_id as f32 * SHARD_WIDTH;
    (left, left + SHARD_WIDTH)
}

fn default_spawn_position(shard_id: u32, client_id: u32) -> Vec2 {
    let (left, right) = shard_bounds(shard_id);
    let lane = (client_id % 6) as f32;
    Vec2::new(left + (right - left) * 0.5, 32.0 + lane * 18.0)
}

fn default_velocity(shard_id: u32, client_id: u32) -> Vec2 {
    let direction = if (shard_id + client_id) % 2 == 0 { 1.0 } else { -1.0 };
    Vec2::new(direction * ENTITY_SPEED, 0.0)
}

fn ensure_player(registry: &mut PlayerRegistry, client_id: u32, shard_id: u32) -> &mut PlayerState {
    registry.players.entry(client_id).or_insert_with(|| PlayerState {
        position: default_spawn_position(shard_id, client_id),
        velocity: default_velocity(shard_id, client_id),
        authority: AuthorityState::Owned,
        handoff_target: None,
        handoff_ticks: 0,
        state_blob: [0u8; STATE_BYTES],
    })
}

fn send_packet(peer: &GamePeer, connection: &GameConnection, stream: &GameStream, packet: Vec<u8>) {
    let _ = peer.send(connection, stream, Bytes::from(packet));
}

fn send_topic_payload(peer: &GamePeer, connection: &GameConnection, stream: &GameStream, topic: &str, payload: &[u8]) {
    send_packet(peer, connection, stream, encode_publish(topic, payload));
}

fn send_subscribe_packet(peer: &GamePeer, connection: &GameConnection, stream: &GameStream, client_id: u32, topic: &str) {
    send_packet(peer, connection, stream, encode_subscribe(client_id, topic));
}

fn broadcast_entity_position(peer: &GamePeer, connection: &GameConnection, stream: &GameStream, shard_id: u32, client_id: u32, entity: &PlayerState) {
    let mut payload = Vec::with_capacity(1 + 4 + 4 + 4);
    payload.push(0x10);
    payload.extend_from_slice(&client_id.to_le_bytes());
    payload.extend_from_slice(&entity.position.x.to_le_bytes());
    payload.extend_from_slice(&entity.position.y.to_le_bytes());
    send_topic_payload(peer, connection, stream, &topic_name(shard_id), &payload);
}

fn parse_broadcast_payload(data: &[u8]) -> Option<&[u8]> {
    if data.len() < 3 || data[0] != 0x04 {
        return None;
    }

    let payload_len = u16::from_le_bytes([data[1], data[2]]) as usize;
    if data.len() < 3 + payload_len {
        return None;
    }

    Some(&data[3..3 + payload_len])
}

fn read_u32(input: &[u8], offset: usize) -> Option<u32> {
    let bytes = input.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_f32(input: &[u8], offset: usize) -> Option<f32> {
    let bytes = input.get(offset..offset + 4)?;
    Some(f32::from_le_bytes(bytes.try_into().ok()?))
}

fn decode_handoff_request(payload: &[u8]) -> Option<(u32, Vec2, Vec2, [u8; STATE_BYTES])> {
    if payload.len() < 1 + 4 + 16 + STATE_BYTES || payload.first().copied()? != 0x20 {
        return None;
    }

    let entity_id = read_u32(payload, 1)?;
    let pos_x = read_f32(payload, 5)?;
    let pos_y = read_f32(payload, 9)?;
    let vel_x = read_f32(payload, 13)?;
    let vel_y = read_f32(payload, 17)?;
    let mut state_blob = [0u8; STATE_BYTES];
    state_blob.copy_from_slice(&payload[21..21 + STATE_BYTES]);

    Some((entity_id, Vec2::new(pos_x, pos_y), Vec2::new(vel_x, vel_y), state_blob))
}

fn position_target_shard(position: Vec2, current_shard: u32) -> Option<u32> {
    let (left, right) = shard_bounds(current_shard);
    if position.x <= left + HANDOFF_MARGIN && current_shard > 0 {
        Some(current_shard - 1)
    } else if position.x >= right - HANDOFF_MARGIN {
        Some(current_shard + 1)
    } else {
        None
    }
}

fn apply_client_input(entity: &mut PlayerState, input: [u8; 16], shard_id: u32) {
    let mut x = (input[0] as f32 / 255.0) * 2.0 - 1.0;
    let mut y = (input[1] as f32 / 255.0) * 2.0 - 1.0;

    if x.abs() < 0.05 {
        x = if shard_id % 2 == 0 { 1.0 } else { -1.0 };
    }

    if y.abs() < 0.05 {
        y = 0.0;
    }

    let direction = Vec2::new(x, y).normalize_or_zero();
    entity.velocity = if direction == Vec2::ZERO {
        default_velocity(shard_id, 0)
    } else {
        direction * ENTITY_SPEED
    };

    entity.state_blob = build_state_blob(entity);
}

fn handle_handoff_request(
    broker: &BrokerConnection,
    registry: &mut PlayerRegistry,
    config: &ServerConfig,
    entity_id: u32,
    position: Vec2,
    velocity: Vec2,
    state_blob: [u8; STATE_BYTES],
) {
    let (left, right) = shard_bounds(config.shard_id);
    let source_shard = if position.x <= left + HANDOFF_MARGIN {
        config.shard_id.saturating_sub(1)
    } else if position.x >= right - HANDOFF_MARGIN {
        config.shard_id + 1
    } else {
        config.shard_id
    };

    let entity = registry.players.entry(entity_id).or_insert(PlayerState {
        position,
        velocity,
        authority: AuthorityState::Ghost,
        handoff_target: Some(source_shard),
        handoff_ticks: 0,
        state_blob,
    });

    entity.position = position;
    entity.velocity = velocity;
    entity.authority = AuthorityState::Ghost;
    entity.handoff_target = Some(source_shard);
    entity.handoff_ticks = 0;
    entity.state_blob = state_blob;

    if let (Some(conn), Some(stream)) = (&broker.connection, &broker.stream) {
        let accept = encode_handoff_ack(0x21, entity_id);
        send_topic_payload(&broker.peer, conn, stream, &topic_name(source_shard), &accept);
    }
}

fn handle_handoff_accept(registry: &mut PlayerRegistry, entity_id: u32) {
    if let Some(entity) = registry.players.get_mut(&entity_id) {
        if entity.authority == AuthorityState::PendingHandoff {
            entity.handoff_ticks = 0;
        }
    }
}

fn handle_handoff_reject(registry: &mut PlayerRegistry, entity_id: u32) {
    if let Some(entity) = registry.players.get_mut(&entity_id) {
        entity.authority = AuthorityState::Owned;
        entity.handoff_target = None;
        entity.handoff_ticks = 0;
        entity.velocity = -entity.velocity;
        entity.state_blob = build_state_blob(entity);
    }
}

fn handle_ghost_update(registry: &mut PlayerRegistry, entity_id: u32, position: Vec2, velocity: Vec2) {
    if let Some(entity) = registry.players.get_mut(&entity_id) {
        if matches!(entity.authority, AuthorityState::Ghost | AuthorityState::PendingHandoff) {
            entity.position = position;
            entity.velocity = velocity;
            entity.state_blob = build_state_blob(entity);
        }
    }
}

fn handle_handoff_complete(registry: &mut PlayerRegistry, entity_id: u32, position: Vec2, velocity: Vec2) {
    if let Some(entity) = registry.players.get_mut(&entity_id) {
        entity.position = position;
        entity.velocity = velocity;
        entity.authority = AuthorityState::Owned;
        entity.handoff_target = None;
        entity.handoff_ticks = 0;
        entity.state_blob = build_state_blob(entity);
    }
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
                if let (Some(conn), Some(stream)) = (&broker.connection, &broker.stream) {
                    send_subscribe_packet(&broker.peer, conn, stream, config.shard_id, &topic_name(config.shard_id));
                }
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

                            let player = ensure_player(&mut registry, client_id, config.shard_id);
                            if data.len() >= 21 {
                                let mut input = [0u8; 16];
                                input.copy_from_slice(&data[5..21]);
                                apply_client_input(player, input, config.shard_id);
                            }

                            println!("Received input from client: {}", client_id);
                        }
                    }
                    0x20 => {
                        if let Some((entity_id, pos, vel, state_blob)) = decode_handoff_request(&data) {
                            handle_handoff_request(&broker, &mut registry, &config, entity_id, pos, vel, state_blob);
                        }
                    }
                    0x21 => {
                        if let Some(entity_id) = read_u32(&data, 1) {
                            handle_handoff_accept(&mut registry, entity_id);
                        }
                    }
                    0x22 => {
                        if let Some(entity_id) = read_u32(&data, 1) {
                            handle_handoff_reject(&mut registry, entity_id);
                        }
                    }
                    0x23 => {
                        if data.len() >= 1 + 4 + 16 {
                            if let Some(entity_id) = read_u32(&data, 1) {
                                let pos_x = read_f32(&data, 5).unwrap_or(0.0);
                                let pos_y = read_f32(&data, 9).unwrap_or(0.0);
                                let vel_x = read_f32(&data, 13).unwrap_or(0.0);
                                let vel_y = read_f32(&data, 17).unwrap_or(0.0);
                                handle_ghost_update(&mut registry, entity_id, Vec2::new(pos_x, pos_y), Vec2::new(vel_x, vel_y));
                            }
                        }
                    }
                    0x24 => {
                        if data.len() >= 1 + 4 + 16 {
                            if let Some(entity_id) = read_u32(&data, 1) {
                                let pos_x = read_f32(&data, 5).unwrap_or(0.0);
                                let pos_y = read_f32(&data, 9).unwrap_or(0.0);
                                let vel_x = read_f32(&data, 13).unwrap_or(0.0);
                                let vel_y = read_f32(&data, 17).unwrap_or(0.0);
                                handle_handoff_complete(&mut registry, entity_id, Vec2::new(pos_x, pos_y), Vec2::new(vel_x, vel_y));
                            }
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
    mut registry: ResMut<PlayerRegistry>,
    config: Res<ServerConfig>,
) {
    if let (Some(conn), Some(stream)) = (&broker.connection, &broker.stream) {
        let player_ids: Vec<u32> = registry.players.keys().copied().collect();
        for client_id in player_ids {
            if let Some(player_state) = registry.players.get_mut(&client_id) {
                let (left, right) = shard_bounds(config.shard_id);

                match player_state.authority {
                    AuthorityState::Owned => {
                        player_state.position += player_state.velocity;

                        if player_state.position.x < left {
                            player_state.position.x = left;
                            player_state.velocity.x = player_state.velocity.x.abs();
                        }

                        if player_state.position.x > right {
                            player_state.position.x = right;
                            player_state.velocity.x = -player_state.velocity.x.abs();
                        }

                        if let Some(target) = position_target_shard(player_state.position, config.shard_id) {
                            player_state.authority = AuthorityState::PendingHandoff;
                            player_state.handoff_target = Some(target);
                            player_state.handoff_ticks = 0;
                            player_state.state_blob = build_state_blob(player_state);

                            if let Some((conn, stream)) = (&broker.connection, &broker.stream) {
                                let request = encode_handoff_request(client_id, player_state);
                                send_topic_payload(&broker.peer, conn, stream, &topic_name(target), &request);
                            }
                        }
                    }
                    AuthorityState::PendingHandoff => {
                        player_state.position += player_state.velocity;
                        player_state.handoff_ticks = player_state.handoff_ticks.saturating_add(1);
                        player_state.state_blob = build_state_blob(player_state);

                        if let Some(target) = player_state.handoff_target {
                            if let Some((conn, stream)) = (&broker.connection, &broker.stream) {
                                let ghost = encode_ghost_update(client_id, player_state);
                                send_topic_payload(&broker.peer, conn, stream, &topic_name(target), &ghost);
                            }

                            if (target > config.shard_id && player_state.position.x >= right - 2.0)
                                || (target < config.shard_id && player_state.position.x <= left + 2.0)
                            {
                                if let Some((conn, stream)) = (&broker.connection, &broker.stream) {
                                    let complete = encode_handoff_complete(client_id);
                                    send_topic_payload(&broker.peer, conn, stream, &topic_name(target), &complete);
                                }

                                player_state.authority = AuthorityState::Ghost;
                                player_state.handoff_ticks = 0;
                            }

                            if player_state.handoff_ticks > 30 {
                                if let Some((conn, stream)) = (&broker.connection, &broker.stream) {
                                    let reject = encode_handoff_ack(0x22, client_id);
                                    send_topic_payload(&broker.peer, conn, stream, &topic_name(target), &reject);
                                }

                                player_state.authority = AuthorityState::Owned;
                                player_state.handoff_target = None;
                                player_state.handoff_ticks = 0;
                                player_state.velocity.x = -player_state.velocity.x;
                            }
                        }
                    }
                    AuthorityState::Ghost => {
                        player_state.state_blob = build_state_blob(player_state);
                    }
                }

                if let (Some(conn), Some(stream)) = (&broker.connection, &broker.stream) {
                    broadcast_entity_position(&broker.peer, conn, stream, config.shard_id, client_id, player_state);
                }
            }
        }
    }
}