use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use redis::Commands;
use std::process::Command;
use shared::Heartbeat;

const ORCH_PORT: &str = "ORCH_PORT";
const HOT_SERVERS_MIN: &str = "HOT_SERVERS_MIN";
const HEARTBEAT_TTL: &str = "HEARTBEAT_TTL";
const SCALER_INTERVAL: &str = "SCALER_INTERVAL";
const REDIS_URL: &str = "REDIS_URL";

struct OrchestratorState {
    next_port: u16,
}

#[tokio::main]
async fn main() {
    let orch_port = std::env::var(ORCH_PORT).unwrap_or_else(|_| "6000".to_string());
    let hot_servers_min: usize = std::env::var(HOT_SERVERS_MIN)
        .unwrap_or_else(|_| "2".to_string())
        .parse()
        .unwrap_or(2);
    let heartbeat_ttl: usize = std::env::var(HEARTBEAT_TTL)
        .unwrap_or_else(|_| "15".to_string())
        .parse()
        .unwrap_or(15);
    let scaler_interval: u64 = std::env::var(SCALER_INTERVAL)
        .unwrap_or_else(|_| "10".to_string())
        .parse()
        .unwrap_or(10);
    let redis_url = std::env::var(REDIS_URL).unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", orch_port))
        .await
        .expect("Failed to bind UDP socket");
    
    let state = Arc::new(Mutex::new(OrchestratorState { next_port: 7000 }));
    
    let socket_listener = Arc::new(socket);

    let redis_listener = redis_url.clone();
    let redis_scaler = redis_url.clone();

    let state_scaler = Arc::clone(&state);

    let heartbeat_task = tokio::spawn(async move {
        heartbeat_listener(socket_listener, redis_listener, heartbeat_ttl).await
    });

    let scaler_task = tokio::spawn(async move {
        scaler_loop(
            state_scaler,
            redis_scaler,
            hot_servers_min,
            scaler_interval,
        )
        .await
    });

    let _ = tokio::join!(heartbeat_task, scaler_task);
}

async fn heartbeat_listener(socket: Arc<UdpSocket>, redis_url: String, ttl: usize) {
    let mut buffer = [0; 1024];
    
    let client = match redis::Client::open(redis_url.as_str()) {
        Ok(c) => c,
        Err(_) => {
            println!("Failed to connect to Redis");
            return;
        }
    };

    let mut conn = match client.get_connection() {
        Ok(c) => c,
        Err(_) => {
            println!("Failed to get Redis connection");
            return;
        }
    };

    loop {
        match socket.recv_from(&mut buffer).await {
            Ok((len, _addr)) => {
                if let Ok(heartbeat_str) = std::str::from_utf8(&buffer[..len]) {
                    if let Ok(heartbeat) = serde_json::from_str::<Heartbeat>(heartbeat_str) {
                        let server_key = format!("server:{}", heartbeat.id);
                        let status = if heartbeat.player_count >= heartbeat.max_players {
                            "full"
                        } else {
                            "available"
                        };
                        
                        let _: Result<(), _> = conn.hset_multiple(
                            &server_key,
                            &[
                                ("id", heartbeat.id),
                                ("ip", heartbeat.ip),
                                ("port", heartbeat.port.to_string()),
                                ("zone", heartbeat.zone),
                                ("player_count", heartbeat.player_count.to_string()),
                                ("max_players", heartbeat.max_players.to_string()),
                                ("status", status.to_string()),
                            ],
                        );

                        let _: Result<(), _> = conn.expire(&server_key, ttl as i64);
                    }
                }
            }
            Err(_) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    }
}

async fn scaler_loop(
    state: Arc<Mutex<OrchestratorState>>,
    redis_url: String,
    hot_servers_min: usize,
    interval_secs: u64,
) {
    let client = match redis::Client::open(redis_url.as_str()) {
        Ok(c) => c,
        Err(_) => {
            println!("Failed to connect to Redis for scaler");
            return;
        }
    };

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

    loop {
        interval.tick().await;

        let mut conn = match client.get_connection() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let available = count_available_servers(&mut conn);

        if available < hot_servers_min {
            let needed = hot_servers_min - available;
            for _ in 0..needed {
                let mut state_guard = state.lock().await;
                spawn_server(state_guard.next_port);
                state_guard.next_port += 1;
            }
        }
    }
}

fn count_available_servers(conn: &mut redis::Connection) -> usize {
    let (_cursor, keys): (u64, Vec<String>) = match redis::cmd("SCAN")
    .arg(0)
    .arg("MATCH")
    .arg("server:*")
    .query(conn)
    {
        Ok(result) => result,
        Err(err) => {
            eprintln!("SCAN error: {:?}", err);
            return 0;
        }
    };

    let mut available = 0;

    for key in keys {
        let status: Result<String, _> = conn.hget(&key, "status");
        match status {
            Ok(s) => {
                if s == "available" {
                    available += 1;
                }
            }
            Err(err) => {
                eprintln!("HGET error for {}: {:?}", key, err);
            }
        }
    }

    available
}

fn spawn_server(port: u16) {
    let _result = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("dedicated_server")
        .env("DS_PORT", port.to_string())
        .spawn();
}
