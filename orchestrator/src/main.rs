use std::collections::HashMap;
use std::env;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;
use tokio::sync::Mutex;

use shared::Heartbeat;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

struct Config {
    orch_port: u16,
    broker_addr: String,
    shard_count: u32,
    ttl: Duration,
    supervise: Duration,
    dummy_shard: Option<u32>,
    redis_url: String,
}

impl Config {
    fn from_env() -> Self {
        Self {
            orch_port: env::var("ORCH_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(6000),
            broker_addr: env::var("BROKER_ADDR").unwrap_or_else(|_| "127.0.0.1:5000".to_string()),
            shard_count: env::var("SHARD_COUNT").ok().and_then(|v| v.parse().ok()).unwrap_or(2),
            ttl: Duration::from_secs(
                env::var("HEARTBEAT_TTL").ok().and_then(|v| v.parse().ok()).unwrap_or(10),
            ),
            supervise: Duration::from_millis(
                env::var("SUPERVISE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(3000),
            ),
            dummy_shard: env::var("SPAWN_DUMMY_SHARD").ok().and_then(|v| v.parse().ok()),
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
        }
    }
}

type Fleet = Arc<Mutex<HashMap<u32, Instant>>>;

#[tokio::main]
async fn main() {
    let config = Arc::new(Config::from_env());

    let redis = redis::Client::open(config.redis_url.as_str())
        .ok()
        .and_then(|c| c.get_connection().ok());
    let redis_ok = redis.is_some();
    let redis = Arc::new(Mutex::new(redis));

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", config.orch_port))
        .await
        .expect("Failed to bind orchestrator UDP socket");
    let socket = Arc::new(socket);

    println!(
        "Orchestrator supervising {} shard(s) on :{} (broker {}, redis: {})",
        config.shard_count,
        config.orch_port,
        config.broker_addr,
        if redis_ok { "connected" } else { "unavailable — in-memory only" }
    );

    let fleet: Fleet = Arc::new(Mutex::new(HashMap::new()));

    let hb_task = tokio::spawn(heartbeat_listener(
        Arc::clone(&socket),
        Arc::clone(&fleet),
        Arc::clone(&redis),
    ));
    let sup_task = tokio::spawn(supervisor(Arc::clone(&config), Arc::clone(&fleet)));

    let _ = tokio::join!(hb_task, sup_task);
}

async fn heartbeat_listener(
    socket: Arc<UdpSocket>,
    fleet: Fleet,
    redis: Arc<Mutex<Option<redis::Connection>>>,
) {
    let mut buf = [0u8; 1024];
    loop {
        let Ok((len, _addr)) = socket.recv_from(&mut buf).await else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&buf[..len]) else { continue };
        let Ok(hb) = serde_json::from_str::<Heartbeat>(text) else { continue };
        let Ok(shard_id) = hb.zone.parse::<u32>() else { continue };

        fleet.lock().await.insert(shard_id, Instant::now());
        println!("Heartbeat from shard {} ({} players)", shard_id, hb.player_count);

        if let Some(conn) = redis.lock().await.as_mut() {
            use redis::Commands;
            let key = format!("shard:{}", shard_id);
            let _: Result<(), _> = conn.hset_multiple(
                &key,
                &[
                    ("id", hb.id),
                    ("zone", hb.zone),
                    ("player_count", hb.player_count.to_string()),
                    ("max_players", hb.max_players.to_string()),
                    ("status", "alive".to_string()),
                ],
            );
            let _: Result<(), _> = conn.expire(&key, 15);
        }
    }
}

async fn supervisor(config: Arc<Config>, fleet: Fleet) {
    let mut interval = tokio::time::interval(config.supervise);
    loop {
        interval.tick().await;
        let now = Instant::now();
        let mut fleet = fleet.lock().await;
        for shard_id in 0..config.shard_count {
            let alive = fleet
                .get(&shard_id)
                .map(|seen| now.duration_since(*seen) <= config.ttl)
                .unwrap_or(false);
            if !alive {
                spawn_shard(&config, shard_id);
                fleet.insert(shard_id, now);
            }
        }
    }
}

fn spawn_shard(config: &Config, shard_id: u32) {
    println!("Shard {} missing — spawning.", shard_id);
    let exe = if cfg!(windows) {
        "target/debug/dedicated_server.exe"
    } else {
        "target/debug/dedicated_server"
    };
    let mut cmd = Command::new(exe);
    cmd.env("SHARD_ID", shard_id.to_string());
    cmd.env("SHARD_COUNT", config.shard_count.to_string());
    cmd.env("BROKER_ADDR", &config.broker_addr);
    cmd.env("ORCH_ADDR", format!("127.0.0.1:{}", config.orch_port));

    if config.dummy_shard == Some(shard_id) {
        cmd.env("SPAWN_DUMMY", "1");
    }
    for key in ["ARENA_WIDTH", "ARENA_HEIGHT", "QUAD_DEPTH"] {
        if let Ok(value) = env::var(key) {
            cmd.env(key, value);
        }
    }

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NEW_CONSOLE);

    if let Err(e) = cmd.spawn() {
        eprintln!("Failed to spawn shard {}: {:?}", shard_id, e);
    }
}
