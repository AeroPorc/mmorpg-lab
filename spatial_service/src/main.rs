use bevy::app::ScheduleRunnerPlugin;
use bevy::prelude::*;
use bytes::Bytes;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GameConnection, GameNetworkEvent, GamePeer, GameStream, GameStreamReliability};
use shared::messages::netmessage::{decode_msg, send_msg, AnyMessage, PubSubMessage, PubSubOp};
use shared::messages::topics::Topic;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::time::Duration;

mod quadtree;
use quadtree::{QuadTree, Rect};

const SHARD_WIDTH: f32 = 256.0;
const WORLD_HEIGHT: f32 = 256.0;
const HANDOFF_MARGIN: f32 = 24.0;
const QUADTREE_MAX_DEPTH: u8 = 2;

const CONTROL_TOPIC: Topic = Topic::View(0);

#[derive(Resource)]
pub struct ServiceConfig {
    pub shard_count: u32,
    pub broker_addr: SocketAddr,
}

impl ServiceConfig {
    fn from_env() -> Self {
        Self {
            shard_count: env::var("SHARD_COUNT").unwrap_or_else(|_| "4".to_string()).parse().unwrap_or(4),
            broker_addr: env::var("BROKER_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:7002".to_string())
                .parse()
                .expect("Invalid broker address"),
        }
    }
}

#[derive(Resource)]
pub struct World {
    pub tree: QuadTree,
    pub entity_shard: HashMap<u32, u32>,
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
    let config = ServiceConfig::from_env();
    let world_bounds = Rect::new(
        Vec2::ZERO,
        Vec2::new(SHARD_WIDTH * config.shard_count as f32, WORLD_HEIGHT),
    );
    let world = World {
        tree: QuadTree::build(world_bounds, QUADTREE_MAX_DEPTH, SHARD_WIDTH),
        entity_shard: HashMap::new(),
    };

    App::new()
        .add_plugins(
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 20.0))),
        )
        .insert_resource(config)
        .insert_resource(world)
        .add_systems(Startup, connect_to_broker)
        .add_systems(Update, (poll_broker, drive_broker_session).chain())
        .run();
}

fn connect_to_broker(mut commands: Commands, config: Res<ServiceConfig>) {
    let peer = GamePeer::new(UdpBackend::new());
    let broker_ip = config.broker_addr.ip().to_string();
    let broker_port = config.broker_addr.port();

    match peer.connect(&broker_ip, broker_port) {
        Ok(_) => println!("Spatial service connecting to broker at {}:{}", broker_ip, broker_port),
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

fn send_control(broker: &BrokerConnection, op: PubSubOp, topic: Topic) {
    send_control_targeted(broker, op, topic, None);
}

fn send_control_targeted(broker: &BrokerConnection, op: PubSubOp, topic: Topic, target: Option<u32>) {
    if let (Some(conn), Some(reliable)) = (&broker.connection, &broker.reliable_stream) {
        let msg = PubSubMessage {
            op,
            topic,
            stream: broker.unreliable_stream.clone(),
            target,
        };
        let _ = send_msg(&broker.peer, conn, reliable, &msg);
    }
}

fn publish_control(broker: &BrokerConnection, payload: &[u8]) {
    if let (Some(conn), Some(stream)) = (&broker.connection, &broker.unreliable_stream) {
        let _ = broker.peer.send(conn, stream, Bytes::copy_from_slice(payload));
    }
}

fn encode_crossing_alert(entity_id: u32, owning_shard: u32, target_shard: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(1 + 4 + 4 + 4);
    payload.push(0x30);
    payload.extend_from_slice(&entity_id.to_le_bytes());
    payload.extend_from_slice(&owning_shard.to_le_bytes());
    payload.extend_from_slice(&target_shard.to_le_bytes());
    payload
}

fn read_u32(input: &[u8], offset: usize) -> Option<u32> {
    let bytes = input.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_f32(input: &[u8], offset: usize) -> Option<f32> {
    let bytes = input.get(offset..offset + 4)?;
    Some(f32::from_le_bytes(bytes.try_into().ok()?))
}

fn handle_position_update(broker: &BrokerConnection, world: &mut World, data: &[u8]) {
    let Some(entity_id) = read_u32(data, 1) else { return };
    let Some(x) = read_f32(data, 5) else { return };
    let Some(y) = read_f32(data, 9) else { return };
    let pos = Vec2::new(x, y);

    let Some(current) = world.tree.shard_for(pos) else { return };

    let previous = world.entity_shard.insert(entity_id, current);
    if previous != Some(current) {
        if let Some(old) = previous {
            send_control_targeted(broker, PubSubOp::StopSub, Topic::Snapshot(old), Some(entity_id));
            println!("Entity {} moved shard {} -> {}: StopSub(Snapshot({})) + Sub(Snapshot({}))", entity_id, old, current, old, current);
        } else {
            println!("Entity {} entered shard {}: Sub(Snapshot({}))", entity_id, current, current);
        }
        send_control_targeted(broker, PubSubOp::Sub, Topic::Snapshot(current), Some(entity_id));
    }

    let near = world.tree.shards_near(pos, HANDOFF_MARGIN);
    if near.len() > 1 {
        if let Some(&target) = near.iter().find(|&&s| s != current) {
            let alert = encode_crossing_alert(entity_id, current, target);
            publish_control(broker, &alert);
            println!("CrossingAlert: entity {} owned by shard {} approaching shard {}", entity_id, current, target);
        }
    }
}

fn handle_control_message(broker: &mut BrokerConnection, data: &Bytes) {
    if data.is_empty() {
        return;
    }

    if let Some(AnyMessage::PubSub(pubsub)) = decode_msg(data) {
        match pubsub.op {
            PubSubOp::ForcedPub => send_control(broker, PubSubOp::Pub, pubsub.topic),
            PubSubOp::ForcedSub => send_control(broker, PubSubOp::Sub, pubsub.topic),
            _ => {}
        }
        return;
    }

    let text = String::from_utf8_lossy(data);
    if text.trim_start().starts_with("WELCOME") {
        println!("Broker welcomed the spatial service.");
        broker.welcomed = true;
    }
}

fn poll_broker(
    mut broker: ResMut<BrokerConnection>,
    mut world: ResMut<World>,
) {
    while let Ok(Some(event)) = broker.peer.poll() {
        match event {
            GameNetworkEvent::Connected(conn) => {
                println!("Connected to broker. Connection ID: {:?}", conn.connection_id);
                broker.connection = Some(conn);
                let _ = broker.peer.create_stream(conn, GameStreamReliability::Reliable);
                let _ = broker.peer.create_stream(conn, GameStreamReliability::Unreliable);
            }
            GameNetworkEvent::StreamCreated(_conn, stream) => {
                if stream.is_reliable() {
                    broker.reliable_stream = Some(stream);
                } else {
                    broker.unreliable_stream = Some(stream);
                }
            }
            GameNetworkEvent::Disconnected(_) => {
                println!("Lost connection to broker.");
                broker.connection = None;
                broker.reliable_stream = None;
                broker.unreliable_stream = None;
                broker.joined = false;
                broker.welcomed = false;
                broker.registered = false;
                world.entity_shard.clear();
            }
            GameNetworkEvent::Message { stream, data, .. } => {
                if stream.is_reliable() {
                    handle_control_message(&mut broker, &data);
                } else if !data.is_empty() && data[0] == 0x10 {
                    handle_position_update(&broker, &mut world, &data);
                }
            }
            GameNetworkEvent::Error { inner, .. } => {
                eprintln!("Broker connection error: {:?}", inner);
            }
            _ => {}
        }
    }
}

fn drive_broker_session(mut broker: ResMut<BrokerConnection>, config: Res<ServiceConfig>) {
    if !broker.joined {
        if let (Some(conn), Some(reliable)) = (&broker.connection, &broker.reliable_stream) {
            let _ = broker.peer.send(conn, reliable, Bytes::from("JOIN spatial"));
            broker.joined = true;
            println!("Sent JOIN to broker as spatial service.");
        }
    }

    if broker.joined && broker.welcomed && !broker.registered && broker.unreliable_stream.is_some() {

        send_control(&broker, PubSubOp::Pub, CONTROL_TOPIC);
        for shard_id in 0..config.shard_count {
            send_control(&broker, PubSubOp::Sub, Topic::Snapshot(shard_id));
        }
        broker.registered = true;
        println!("Spatial service registered: publishing {:?}, subscribed to {} shard snapshots.", CONTROL_TOPIC, config.shard_count);
    }
}
