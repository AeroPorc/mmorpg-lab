use shared::*;
use bevy::prelude::*;

use crate::AppState;
use crate::GameServerInfo;

// A simple plugin that handle the login to the gatekeeper
// Override GameServerInfo's ressource for the initialisation of the network plugin (indicate ip and port to connect to)

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

        // Login done, initiate the connection to the game server
        commands.insert_resource(
            GameServerInfo(response.server)
        );

        next_state.set(AppState::Connecting);
    });
}