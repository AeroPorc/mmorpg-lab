pub mod messages;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Heartbeat {
    pub id: String,
    pub ip: String,
    pub port: u16,
    pub zone: String,
    pub player_count: usize,
    pub max_players: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerInfo {
    pub ip: String,
    pub port: u16,
    pub zone: String,
}

impl ServerInfo {
    pub fn base(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.base())
    }

    pub fn http_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginRequest {
    pub username: String,
    pub password: String, // useless, must be "1234" to enable connection to the Gatekeeper
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoginResponse {
    pub player_id: String,
    pub server: ServerInfo,
}