use shared::*;
use bevy::prelude::*;

use crate::AppState;
use crate::GameServerInfo;
use crate::network::LocalPlayer;

fn player_id_to_u32(s: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in s.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}



#[derive(Resource, Clone)]
pub struct GatekeeperInfo(pub ServerInfo);

impl GatekeeperInfo {
    pub fn login_url(&self) -> String {
        self.0.http_url("/login")
    }

    pub fn health_url(&self) -> String {
        self.0.http_url("/health")
    }
}

pub struct GatekeeperLoginPlugin;

impl Plugin for GatekeeperLoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Login), start_login);
        app.add_systems(OnEnter(AppState::Rejected), reattempt_login);
    }
}

fn start_login(
    gatekeeper: ResMut<GatekeeperInfo>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");

    runtime.block_on(async move {
        // Requête GET /health
        println!("Login attempt...");

        println!("Is Gatekeeper alive ?");

        let response = reqwest::get(gatekeeper.health_url())
            .await
            .expect("Health request failed")
            .text()
            .await
            .expect("Invalid health response");

        println!("Response from gatekeeper: {}", response);

        // POST /login (vrai login)
        let client = reqwest::Client::new();

        let response = client
            .post(gatekeeper.login_url())
            .json(&LoginRequest {
                username: String::from("Caillou"),
                password: String::from("1234"),
            })
            .send()
            .await
            .expect("Login request failed")
            .json::<LoginResponse>()
            .await
            .expect("Invalid login response");

        println!("Logged in successfully!");
        println!("player_id: {}", response.player_id);
        println!(
            "Game server: {}.{} -> {}",
            response.server.ip, response.server.port, response.server.zone
        );

        commands.insert_resource(LocalPlayer {
            id: player_id_to_u32(&response.player_id),
        });
        commands.insert_resource(
            GameServerInfo(response.server)
        );

        next_state.set(AppState::Connecting);
    });
}

fn reattempt_login(
    gatekeeper: ResMut<GatekeeperInfo>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut game_server: ResMut<GameServerInfo>,
) {
    let runtime = tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime");

    runtime.block_on(async move {
        println!("Login reattempt...");

        let client = reqwest::Client::new();

        let response = client
            .post(gatekeeper.login_url())
            .json(&LoginRequest {
                username: String::from("Caillou"),
                password: String::from("1234"),
            })
            .send()
            .await
            .expect("Login request failed")
            .json::<LoginResponse>()
            .await
            .expect("Invalid login response");

        println!("Logged in successfully!");
        println!("New player_id: {}", response.player_id);
        println!(
            "New Game server: {}.{} -> {}",
            response.server.ip, response.server.port, response.server.zone
        );

        commands.insert_resource(LocalPlayer {
            id: player_id_to_u32(&response.player_id),
        });
        *game_server = GameServerInfo(response.server);

        next_state.set(AppState::Connecting);
    });
}