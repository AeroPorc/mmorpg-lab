use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bytes::Bytes;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GameConnection, GameNetworkEvent, GamePeer, GameStream, GameStreamReliability};
use shared::messages::netmessage::{decode_msg, send_msg, AnyMessage, PubSubMessage, PubSubOp};
use shared::messages::topics::Topic;
use shared::spatial::{QuadTree, Rect};
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use uuid::Uuid;

const TICK_HZ: f64 = 20.0;

// Arena defaults (overridable via env: ARENA_WIDTH / ARENA_HEIGHT / QUAD_DEPTH).
const DEFAULT_SHARD_WIDTH: f32 = 256.0;
const DEFAULT_WORLD_HEIGHT: f32 = 256.0;
const DEFAULT_QUAD_DEPTH: u8 = 2;

const HANDOFF_MARGIN: f32 = 24.0;
const DUMMY_SPEED: f32 = 1.0;
const PLAYER_SPEED: f32 = 6.0;
const STATE_BYTES: usize = 64;

// Enemy AI tuning.
const ENEMY_SPEED: f32 = 6.0;
const SPAWN_INTERVAL_TICKS: u32 = 20;
const SPAWN_BATCH: usize = 8;
const MAX_ENEMIES: usize = 200;
// Enemies spawn on a ring this far from the player (clamped to its leaf cell).
const ENEMY_SPAWN_RADIUS: f32 = 280.0;

// Combat tuning.
const PLAYER_MAX_HP: i32 = 100;
const ENEMY_BASE_HP: i32 = 3;
const ENEMY_CONTACT_DAMAGE: i32 = 6;
const CONTACT_RANGE: f32 = 16.0;
const PLAYER_DAMAGE_COOLDOWN: u8 = 12; // i-frames (~0.6s)
const ATTACK_COOLDOWN: u8 = 6; // auto-attack every ~0.3s
const ATTACK_RANGE: f32 = 130.0;
const ATTACK_DAMAGE: i32 = 1;

// Waves: difficulty steps up every interval.
const WAVE_INTERVAL_TICKS: u32 = 300; // ~15s

#[derive(Resource)]
pub struct ServerConfig {
    pub id: String,
    pub shard_id: u32,
    pub shard_count: u32,
    pub shard_width: f32,
    pub world_height: f32,
    pub quad_depth: u8,
    pub broker_addr: SocketAddr,
}

impl ServerConfig {
    fn from_env() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            shard_id: env::var("SHARD_ID").unwrap_or_else(|_| "0".to_string()).parse().unwrap(),
            shard_count: env::var("SHARD_COUNT").unwrap_or_else(|_| "4".to_string()).parse().unwrap_or(4),
            shard_width: env::var("ARENA_WIDTH").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_SHARD_WIDTH),
            world_height: env::var("ARENA_HEIGHT").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_WORLD_HEIGHT),
            quad_depth: env::var("QUAD_DEPTH").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_QUAD_DEPTH),
            broker_addr: env::var("BROKER_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:7002".to_string())
                .parse()
                .expect("Invalid broker address"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityState {
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
    pub bounce: bool,
    pub hp: i32,
    pub score: u32,
    pub attack_cd: u8,
    pub damage_cd: u8,
}

impl PlayerState {
    fn fresh(position: Vec2, velocity: Vec2, bounce: bool) -> Self {
        Self {
            position,
            velocity,
            authority: AuthorityState::Owned,
            handoff_target: None,
            handoff_ticks: 0,
            state_blob: [0u8; STATE_BYTES],
            bounce,
            hp: PLAYER_MAX_HP,
            score: 0,
            attack_cd: 0,
            damage_cd: 0,
        }
    }
}

#[derive(Resource, Default)]
pub struct PlayerRegistry {
    pub players: HashMap<u32, PlayerState>,
}

pub struct Enemy {
    pub position: Vec2,
    pub velocity: Vec2,
    pub target: Option<u32>,
    pub hp: i32,
}

/// Wave / difficulty progression.
#[derive(Resource, Default)]
pub struct GameState {
    pub wave: u32,
    pub wave_clock: u32,
}

#[derive(Resource)]
pub struct EnemyRegistry {
    pub enemies: HashMap<u32, Enemy>,
    pub next_id: u32,
    pub spawn_clock: u32,
    pub rng: u32,
}

impl Default for EnemyRegistry {
    fn default() -> Self {
        Self {
            enemies: HashMap::new(),
            next_id: 1,
            spawn_clock: 0,
            rng: 0x9E37_79B9,
        }
    }
}


#[derive(Resource)]
pub struct ShardWorld {
    pub tree: QuadTree,
}

impl ShardWorld {
    fn new(config: &ServerConfig) -> Self {
        let bounds = Rect::new(
            Vec2::ZERO,
            Vec2::new(config.shard_width * config.shard_count as f32, config.world_height),
        );
        Self {
            tree: QuadTree::build(bounds, config.quad_depth, config.shard_width),
        }
    }
}

fn next_rand(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

fn rand_unit(state: &mut u32) -> f32 {
    next_rand(state) as f32 / u32::MAX as f32
}

#[derive(Resource)]
pub struct BrokerConnection {
    pub peer: GamePeer,
    pub connection: Option<GameConnection>,
    pub reliable_stream: Option<GameStream>,
    pub unreliable_stream: Option<GameStream>,
    pub joined: bool,
    pub welcomed: bool,
    pub registered: bool,
}

fn main() {
    let config = ServerConfig::from_env();
    let world = ShardWorld::new(&config);

    App::new()
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / TICK_HZ))),
        )
        .insert_resource(config)
        .insert_resource(world)
        .init_resource::<PlayerRegistry>()
        .init_resource::<EnemyRegistry>()
        .init_resource::<GameState>()
        .add_systems(Startup, (connect_to_broker, maybe_spawn_test_entity))
        .add_systems(
            Update,
            (
                poll_broker,
                drive_broker_session,
                simulate_and_publish,
                advance_waves,
                spawn_enemies,
                update_and_cull_enemies,
                combat,
            )
                .chain(),
        )
        .run();
}

fn maybe_spawn_test_entity(mut registry: ResMut<PlayerRegistry>, config: Res<ServerConfig>) {
    if env::var("SPAWN_DUMMY").is_err() {
        return;
    }

    let client_id = 100 * config.shard_id + 1;
    let position = default_spawn_position(config.shard_id, client_id, config.shard_width);
    registry.players.insert(
        client_id,
        PlayerState::fresh(position, Vec2::new(DUMMY_SPEED, 0.0), true),
    );
    println!("Spawned test entity {} at {:?} (SPAWN_DUMMY).", client_id, position);
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
        reliable_stream: None,
        unreliable_stream: None,
        joined: false,
        welcomed: false,
        registered: false,
    });
}


fn publish_gameplay(broker: &BrokerConnection, payload: &[u8]) {
    if let (Some(conn), Some(stream)) = (&broker.connection, &broker.unreliable_stream) {
        let _ = broker.peer.send(conn, stream, Bytes::copy_from_slice(payload));
    }
}

fn send_control(broker: &BrokerConnection, op: PubSubOp, topic: Topic) {
    if let (Some(conn), Some(reliable)) = (&broker.connection, &broker.reliable_stream) {
        let msg = PubSubMessage {
            op,
            topic,
            stream: broker.unreliable_stream.clone(),
        };
        let _ = send_msg(&broker.peer, conn, reliable, &msg);
    }
}

fn publish_entity_position(broker: &BrokerConnection, client_id: u32, entity: &PlayerState) {
    let mut payload = Vec::with_capacity(1 + 4 + 4 + 4);
    payload.push(0x10);
    payload.extend_from_slice(&client_id.to_le_bytes());
    payload.extend_from_slice(&entity.position.x.to_le_bytes());
    payload.extend_from_slice(&entity.position.y.to_le_bytes());
    publish_gameplay(broker, &payload);
}

fn publish_enemy_update(broker: &BrokerConnection, enemy_id: u32, pos: Vec2) {
    let mut payload = Vec::with_capacity(1 + 4 + 4 + 4);
    payload.push(0x40);
    payload.extend_from_slice(&enemy_id.to_le_bytes());
    payload.extend_from_slice(&pos.x.to_le_bytes());
    payload.extend_from_slice(&pos.y.to_le_bytes());
    publish_gameplay(broker, &payload);
}

fn publish_enemy_despawn(broker: &BrokerConnection, enemy_id: u32) {
    let mut payload = Vec::with_capacity(1 + 4);
    payload.push(0x41);
    payload.extend_from_slice(&enemy_id.to_le_bytes());
    publish_gameplay(broker, &payload);
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


fn shard_bounds(shard_id: u32, shard_width: f32) -> (f32, f32) {
    let left = shard_id as f32 * shard_width;
    (left, left + shard_width)
}

fn default_spawn_position(shard_id: u32, client_id: u32, shard_width: f32) -> Vec2 {
    let (left, right) = shard_bounds(shard_id, shard_width);
    let lane = (client_id % 6) as f32;
    Vec2::new(left + (right - left) * 0.5, 32.0 + lane * 18.0)
}

fn ensure_player<'a>(
    registry: &'a mut PlayerRegistry,
    client_id: u32,
    shard_id: u32,
    shard_width: f32,
) -> &'a mut PlayerState {
    registry.players.entry(client_id).or_insert_with(|| {
        PlayerState::fresh(default_spawn_position(shard_id, client_id, shard_width), Vec2::ZERO, false)
    })
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

fn position_target_shard(position: Vec2, current_shard: u32, shard_count: u32, shard_width: f32) -> Option<u32> {
    let (left, right) = shard_bounds(current_shard, shard_width);
    if position.x <= left + HANDOFF_MARGIN && current_shard > 0 {
        Some(current_shard - 1)
    } else if position.x >= right - HANDOFF_MARGIN && current_shard + 1 < shard_count {
        Some(current_shard + 1)
    } else {
        None
    }
}
fn moving_toward(velocity: Vec2, current_shard: u32, target_shard: u32) -> bool {
    if target_shard > current_shard {
        velocity.x > 0.0
    } else {
        velocity.x < 0.0
    }
}

fn apply_client_input(entity: &mut PlayerState, input: [u8; 16], _shard_id: u32) {
    let x = (input[0] as f32 / 255.0) * 2.0 - 1.0;
    let y = (input[1] as f32 / 255.0) * 2.0 - 1.0;

    let direction = Vec2::new(x, y);
    entity.velocity = if direction.length() < 0.1 {
        Vec2::ZERO 
    } else {
        direction.normalize() * PLAYER_SPEED
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
    let (left, right) = shard_bounds(config.shard_id, config.shard_width);
    let source_shard = if position.x <= left + HANDOFF_MARGIN {
        config.shard_id.saturating_sub(1)
    } else if position.x >= right - HANDOFF_MARGIN {
        config.shard_id + 1
    } else {
        config.shard_id
    };

    let entity = registry
        .players
        .entry(entity_id)
        .or_insert_with(|| PlayerState::fresh(position, velocity, true));

    entity.position = position;
    entity.velocity = velocity;
    entity.authority = AuthorityState::Ghost;
    entity.handoff_target = Some(source_shard);
    entity.handoff_ticks = 0;
    entity.state_blob = state_blob;

    let accept = encode_handoff_ack(0x21, entity_id);
    publish_gameplay(broker, &accept);
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

fn handle_handoff_complete(registry: &mut PlayerRegistry, entity_id: u32) {
    
    if let Some(entity) = registry.players.get_mut(&entity_id) {
        entity.authority = AuthorityState::Owned;
        entity.handoff_target = None;
        entity.handoff_ticks = 0;
        entity.state_blob = build_state_blob(entity);
    }
}

fn handle_crossing_alert(
    broker: &BrokerConnection,
    registry: &mut PlayerRegistry,
    config: &ServerConfig,
    entity_id: u32,
    owning_shard: u32,
    target_shard: u32,
) {
    if owning_shard != config.shard_id {
        return;
    }

    let request = match registry.players.get_mut(&entity_id) {
        Some(entity)
            if entity.authority == AuthorityState::Owned
                && moving_toward(entity.velocity, owning_shard, target_shard) =>
        {
            entity.authority = AuthorityState::PendingHandoff;
            entity.handoff_target = Some(target_shard);
            entity.handoff_ticks = 0;
            entity.state_blob = build_state_blob(entity);
            encode_handoff_request(entity_id, entity)
        }
        _ => return,
    };

    publish_gameplay(broker, &request);
}

fn handle_control_message(broker: &mut BrokerConnection, data: &Bytes) {
    if data.is_empty() {
        return;
    }

    if let Some(AnyMessage::PubSub(pubsub)) = decode_msg(data) {
        match pubsub.op {
            PubSubOp::ForcedPub => {
                println!("Broker forced publish of a topic.");
                send_control(broker, PubSubOp::Pub, pubsub.topic);
            }
            PubSubOp::ForcedSub => {
                println!("Broker forced subscription to a topic.");
                send_control(broker, PubSubOp::Sub, pubsub.topic);
            }
            _ => {}
        }
        return;
    }

    let text = String::from_utf8_lossy(data);
    if text.trim_start().starts_with("WELCOME") {
        println!("Broker welcomed the shard.");
        broker.welcomed = true;
    }
}

fn handle_gameplay_message(
    broker: &BrokerConnection,
    registry: &mut PlayerRegistry,
    config: &ServerConfig,
    data: &Bytes,
) {
    if data.is_empty() {
        return;
    }

    match data[0] {
        0x05 => {
            if data.len() >= 5 {
                let client_id = u32::from_le_bytes(data[1..5].try_into().unwrap());

                let player = ensure_player(registry, client_id, config.shard_id, config.shard_width);
                if data.len() >= 21 {
                    let mut input = [0u8; 16];
                    input.copy_from_slice(&data[5..21]);
                    apply_client_input(player, input, config.shard_id);
                }

                println!("Received input from client: {}", client_id);
            }
        }
        0x20 => {
            if let Some((entity_id, pos, vel, state_blob)) = decode_handoff_request(data) {
                handle_handoff_request(broker, registry, config, entity_id, pos, vel, state_blob);
            }
        }
        0x21 => {
            if let Some(entity_id) = read_u32(data, 1) {
                handle_handoff_accept(registry, entity_id);
            }
        }
        0x22 => {
            if let Some(entity_id) = read_u32(data, 1) {
                handle_handoff_reject(registry, entity_id);
            }
        }
        0x23 => {
            if data.len() >= 1 + 4 + 16 {
                if let Some(entity_id) = read_u32(data, 1) {
                    let pos_x = read_f32(data, 5).unwrap_or(0.0);
                    let pos_y = read_f32(data, 9).unwrap_or(0.0);
                    let vel_x = read_f32(data, 13).unwrap_or(0.0);
                    let vel_y = read_f32(data, 17).unwrap_or(0.0);
                    handle_ghost_update(registry, entity_id, Vec2::new(pos_x, pos_y), Vec2::new(vel_x, vel_y));
                }
            }
        }
        0x24 => {
            if let Some(entity_id) = read_u32(data, 1) {
                handle_handoff_complete(registry, entity_id);
            }
        }
        0x30 => {
            if let (Some(entity_id), Some(owning), Some(target)) =
                (read_u32(data, 1), read_u32(data, 5), read_u32(data, 9))
            {
                handle_crossing_alert(broker, registry, config, entity_id, owning, target);
            }
        }
        0x10 | 0x11 | 0x40 | 0x41 => {
            // Neighbour world / enemy / stats traffic on subscribed snapshot topics; ignore.
        }
        tag => {
            println!("Received unknown gameplay tag: {}", tag);
        }
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
                let _ = broker.peer.create_stream(conn, GameStreamReliability::Reliable);
                let _ = broker.peer.create_stream(conn, GameStreamReliability::Unreliable);
            }
            GameNetworkEvent::StreamCreated(_conn, stream) => {
                if stream.is_reliable() {
                    println!("Broker lifeline (reliable) stream ready.");
                    broker.reliable_stream = Some(stream);
                } else {
                    println!("Broker gameplay (unreliable) stream ready.");
                    broker.unreliable_stream = Some(stream);
                }
            }
            GameNetworkEvent::Disconnected(_) => {
                println!("Lost connection to Broker.");
                broker.connection = None;
                broker.reliable_stream = None;
                broker.unreliable_stream = None;
                broker.joined = false;
                broker.welcomed = false;
                broker.registered = false;
            }
            GameNetworkEvent::Message { stream, data, .. } => {
                if stream.is_reliable() {
                    handle_control_message(&mut broker, &data);
                } else {
                    handle_gameplay_message(&broker, &mut registry, &config, &data);
                }
            }
            GameNetworkEvent::Error { inner, .. } => {
                eprintln!("Broker connection error: {:?}", inner);
            }
            _ => {}
        }
    }
}

fn drive_broker_session(mut broker: ResMut<BrokerConnection>, config: Res<ServerConfig>) {
    if !broker.joined {
        if let (Some(conn), Some(reliable)) = (&broker.connection, &broker.reliable_stream) {
            let join = format!("JOIN shard {}", config.shard_id);
            let _ = broker.peer.send(conn, reliable, Bytes::from(join));
            broker.joined = true;
            println!("Sent JOIN to broker as shard {}.", config.shard_id);
        }
    }

    if broker.joined && broker.welcomed && !broker.registered && broker.unreliable_stream.is_some() {
        send_control(&broker, PubSubOp::Pub, Topic::Snapshot(config.shard_id));

        if config.shard_id > 0 {
            send_control(&broker, PubSubOp::Sub, Topic::Snapshot(config.shard_id - 1));
        }
        send_control(&broker, PubSubOp::Sub, Topic::Snapshot(config.shard_id + 1));

        send_control(&broker, PubSubOp::Sub, Topic::View(0));

        send_control(&broker, PubSubOp::Sub, Topic::Input(0));

        broker.registered = true;
        println!("Registered pub/sub with broker.");
    }
}

fn simulate_and_publish(
    broker: Res<BrokerConnection>,
    mut registry: ResMut<PlayerRegistry>,
    config: Res<ServerConfig>,
) {
    if broker.connection.is_none() || broker.unreliable_stream.is_none() || !broker.registered {
        return;
    }

    let player_ids: Vec<u32> = registry.players.keys().copied().collect();
    for client_id in player_ids {
        if let Some(player_state) = registry.players.get_mut(&client_id) {
            let (left, right) = shard_bounds(config.shard_id, config.shard_width);

            match player_state.authority {
                AuthorityState::Owned => {
                    player_state.position += player_state.velocity;

                    if player_state.position.x < left {
                        player_state.position.x = left;
                        player_state.velocity.x =
                            if player_state.bounce { player_state.velocity.x.abs() } else { 0.0 };
                    }
                    if player_state.position.x > right {
                        player_state.position.x = right;
                        player_state.velocity.x =
                            if player_state.bounce { -player_state.velocity.x.abs() } else { 0.0 };
                    }

                    if player_state.position.y < 0.0 {
                        player_state.position.y = 0.0;
                        player_state.velocity.y =
                            if player_state.bounce { player_state.velocity.y.abs() } else { 0.0 };
                    }
                    if player_state.position.y > config.world_height {
                        player_state.position.y = config.world_height;
                        player_state.velocity.y =
                            if player_state.bounce { -player_state.velocity.y.abs() } else { 0.0 };
                    }

                    if let Some(target) = position_target_shard(
                        player_state.position,
                        config.shard_id,
                        config.shard_count,
                        config.shard_width,
                    ) {
                        if moving_toward(player_state.velocity, config.shard_id, target) {
                            player_state.authority = AuthorityState::PendingHandoff;
                            player_state.handoff_target = Some(target);
                            player_state.handoff_ticks = 0;
                            player_state.state_blob = build_state_blob(player_state);

                            let request = encode_handoff_request(client_id, player_state);
                            publish_gameplay(&broker, &request);
                        }
                    }
                }
                AuthorityState::PendingHandoff => {
                    player_state.position += player_state.velocity;
                    player_state.handoff_ticks = player_state.handoff_ticks.saturating_add(1);
                    player_state.state_blob = build_state_blob(player_state);

                    if let Some(target) = player_state.handoff_target {
                        let ghost = encode_ghost_update(client_id, player_state);
                        publish_gameplay(&broker, &ghost);

                        if (target > config.shard_id && player_state.position.x >= right - 2.0)
                            || (target < config.shard_id && player_state.position.x <= left + 2.0)
                        {
                            let complete = encode_handoff_complete(client_id);
                            publish_gameplay(&broker, &complete);

                            player_state.authority = AuthorityState::Ghost;
                            player_state.handoff_ticks = 0;
                        }

                        if player_state.handoff_ticks > 30 {
                            let reject = encode_handoff_ack(0x22, client_id);
                            publish_gameplay(&broker, &reject);

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

            if player_state.authority != AuthorityState::Ghost {
                publish_entity_position(&broker, client_id, player_state);
            }
        }
    }
}


fn nearest_owned_player(registry: &PlayerRegistry, pos: Vec2) -> Option<(u32, Vec2)> {
    registry
        .players
        .iter()
        .filter(|(_, p)| p.authority == AuthorityState::Owned)
        .min_by(|a, b| {
            let da = a.1.position.distance_squared(pos);
            let db = b.1.position.distance_squared(pos);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(&id, p)| (id, p.position))
}

fn spawn_enemies(
    mut enemies: ResMut<EnemyRegistry>,
    players: Res<PlayerRegistry>,
    world: Res<ShardWorld>,
    game: Res<GameState>,
    config: Res<ServerConfig>,
) {
    enemies.spawn_clock += 1;
    if enemies.spawn_clock < SPAWN_INTERVAL_TICKS {
        return;
    }
    enemies.spawn_clock = 0;

    if enemies.enemies.len() >= MAX_ENEMIES {
        return;
    }

    let Some((target_id, target_pos)) = players
        .players
        .iter()
        .find(|(_, p)| p.authority == AuthorityState::Owned)
        .map(|(&id, p)| (id, p.position))
    else {
        return;
    };

    let Some(leaf) = world.tree.leaf_for(target_pos) else {
        return;
    };

    // Spawn on a ring around the player, but keep it inside the leaf cell so the
    // enemies survive the leaf cull. (In a small leaf the ring shrinks to fit.)
    let leaf_half = (leaf.max.x - leaf.min.x).min(leaf.max.y - leaf.min.y) * 0.5;
    let radius = ENEMY_SPAWN_RADIUS.min(leaf_half * 0.9).max(8.0);

    // Wave scaling: bigger batches and tougher enemies as waves progress.
    let batch = (SPAWN_BATCH + game.wave as usize).min(MAX_ENEMIES - enemies.enemies.len());
    let enemy_hp = ENEMY_BASE_HP + game.wave as i32 / 2;

    for _ in 0..batch {
        let id = enemies.next_id;
        enemies.next_id += 1;
        let angle = rand_unit(&mut enemies.rng) * std::f32::consts::TAU;
        let raw = target_pos + Vec2::new(angle.cos(), angle.sin()) * radius;
        let position = Vec2::new(
            raw.x.clamp(leaf.min.x, leaf.max.x),
            raw.y.clamp(leaf.min.y, leaf.max.y),
        );
        enemies.enemies.insert(
            id,
            Enemy {
                position,
                velocity: Vec2::ZERO,
                target: Some(target_id),
                hp: enemy_hp,
            },
        );
    }

    println!(
        "Shard {}: wave {} spawned {} enemies (total {})",
        config.shard_id,
        game.wave + 1,
        batch,
        enemies.enemies.len()
    );
}


fn update_and_cull_enemies(
    broker: Res<BrokerConnection>,
    mut enemies: ResMut<EnemyRegistry>,
    players: Res<PlayerRegistry>,
    world: Res<ShardWorld>,
    config: Res<ServerConfig>,
) {
    if broker.connection.is_none() || broker.unreliable_stream.is_none() || !broker.registered {
        return;
    }

    let mut culled: Vec<u32> = Vec::new();
    let enemy_ids: Vec<u32> = enemies.enemies.keys().copied().collect();

    for id in enemy_ids {
        let enemy_pos = enemies.enemies[&id].position;

        let Some((player_id, player_pos)) = nearest_owned_player(&players, enemy_pos) else {
            culled.push(id);
            continue;
        };

        if !world.tree.same_leaf(enemy_pos, player_pos) {
            culled.push(id);
            continue;
        }

        let enemy = enemies.enemies.get_mut(&id).unwrap();
        enemy.target = Some(player_id);
        let direction = (player_pos - enemy.position).normalize_or_zero();
        enemy.velocity = direction * ENEMY_SPEED;
        enemy.position += enemy.velocity;
        let pos = enemy.position;
        publish_enemy_update(&broker, id, pos);
    }

    if !culled.is_empty() {
        for id in &culled {
            enemies.enemies.remove(id);
            publish_enemy_despawn(&broker, *id);
        }
        println!(
            "Shard {}: culled {} enemies outside player leaf ({} remaining)",
            config.shard_id,
            culled.len(),
            enemies.enemies.len()
        );
    }
}

fn advance_waves(mut game: ResMut<GameState>) {
    game.wave_clock += 1;
    if game.wave_clock >= WAVE_INTERVAL_TICKS {
        game.wave_clock = 0;
        game.wave += 1;
        println!("=== Wave {} ===", game.wave + 1);
    }
}

fn publish_player_stats(broker: &BrokerConnection, client_id: u32, hp: i32, score: u32, wave: u32) {
    let mut payload = Vec::with_capacity(1 + 4 + 4 + 4 + 4);
    payload.push(0x11);
    payload.extend_from_slice(&client_id.to_le_bytes());
    payload.extend_from_slice(&hp.to_le_bytes());
    payload.extend_from_slice(&score.to_le_bytes());
    payload.extend_from_slice(&wave.to_le_bytes());
    publish_gameplay(broker, &payload);
}

/// Player auto-attacks the nearest enemy in range; enemies deal contact damage;
/// dead enemies award score; a dead player respawns. Publishes per-player stats.
fn combat(
    broker: Res<BrokerConnection>,
    mut players: ResMut<PlayerRegistry>,
    mut enemies: ResMut<EnemyRegistry>,
    game: Res<GameState>,
) {
    if broker.connection.is_none() || broker.unreliable_stream.is_none() || !broker.registered {
        return;
    }

    let player_ids: Vec<u32> = players.players.keys().copied().collect();
    for pid in player_ids {
        let Some((ppos, authority, mut atk_cd, mut dmg_cd)) = players
            .players
            .get(&pid)
            .map(|p| (p.position, p.authority, p.attack_cd, p.damage_cd))
        else {
            continue;
        };
        if authority != AuthorityState::Owned {
            continue;
        }

        atk_cd = atk_cd.saturating_sub(1);
        dmg_cd = dmg_cd.saturating_sub(1);

        let mut score_gain: u32 = 0;
        let mut hp_delta: i32 = 0;

        // Auto-attack: hit the nearest enemy within range.
        if atk_cd == 0 {
            let mut best: Option<(u32, f32)> = None;
            for (&eid, e) in enemies.enemies.iter() {
                let d = e.position.distance_squared(ppos);
                if d <= ATTACK_RANGE * ATTACK_RANGE && best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((eid, d));
                }
            }
            if let Some((eid, _)) = best {
                atk_cd = ATTACK_COOLDOWN;
                if let Some(e) = enemies.enemies.get_mut(&eid) {
                    e.hp -= ATTACK_DAMAGE;
                    if e.hp <= 0 {
                        enemies.enemies.remove(&eid);
                        publish_enemy_despawn(&broker, eid);
                        score_gain += 1;
                    }
                }
            }
        }

        // Contact damage: any enemy touching the player hurts (with i-frames).
        if dmg_cd == 0 {
            let touched = enemies
                .enemies
                .values()
                .any(|e| e.position.distance_squared(ppos) <= CONTACT_RANGE * CONTACT_RANGE);
            if touched {
                hp_delta -= ENEMY_CONTACT_DAMAGE;
                dmg_cd = PLAYER_DAMAGE_COOLDOWN;
            }
        }

        if let Some(p) = players.players.get_mut(&pid) {
            p.attack_cd = atk_cd;
            p.damage_cd = dmg_cd;
            p.score += score_gain;
            p.hp += hp_delta;
            if p.hp <= 0 {
                p.hp = PLAYER_MAX_HP;
                p.score = 0;
                println!("Player {} died — respawning.", pid);
            }
            publish_player_stats(&broker, pid, p.hp, p.score, game.wave + 1);
        }
    }
}
