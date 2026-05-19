use shared::*;
use bevy::prelude::*;

mod state;
use crate::state::AppState;

mod login;
use crate::login::*;

mod network;
use crate::network::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gatekeeper = GatekeeperInfo {
        0: ServerInfo {
            ip: String::from("127.0.0.1"),
            port: 3000,
            zone: String::from("N/A"),
        }
    };

    // Requête GET /health
    println!("Is Gatekeeper alive ?");

    let response = reqwest::get(gatekeeper.health_url())
        .await?
        .text()
        .await?;

    println!("Response from gatekeeper: {}", response);

    // POST /login (vrai login)
    let client = reqwest::Client::new();

    let response = client
        .post(gatekeeper.login_url())
        .json(&LoginRequest {
            username: String::from("Pierre"),
            password: String::from("1234"),
        })
        .send()
        .await?
        .json::<LoginResponse>()
        .await?;

    println!("Logged in successfully!");
    println!("player_id: {}", response.player_id);
    println!(
        "Game server: {}.{} -> {}",
        response.server.ip, response.server.port, response.server.zone
    );

    App::new()
        .add_plugins(DefaultPlugins)
        //.add_plugins(GatekeeperLoginPlugin)
        .add_plugins(NetworkPlugin)
        .init_state::<AppState>()
        .insert_resource(GameServerInfo {
            0: response.server
        })
        .insert_state(AppState::Connecting)
        //.add_systems(Update, handle_network)
        .run();

    Ok(())
}