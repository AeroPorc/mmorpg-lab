use shared::*;
use bevy::prelude::*;

mod state;
use crate::state::AppState;

mod login;
use crate::login::*;

mod network;
use crate::network::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GatekeeperLoginPlugin)
        .add_plugins(NetworkPlugin)
        .init_state::<AppState>()
        .insert_resource(GatekeeperInfo { 
            0: ServerInfo {
                ip: "127.0.0.1".to_string(),
                port: 3000,
                zone: "N/A".to_string(),
            },
        })
        .insert_state(AppState::Login)
        //.add_systems(Update, handle_network)
        .run();

    Ok(())
}