use shared::*;
use bevy::prelude::*;
//use bevy::ecs::event::Event;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GamePeer, GameNetworkEvent, GameConnection, GameStream, GameStreamReliability};

//use bytes::Bytes;
//use uuid::Uuid;

use crate::AppState;

#[derive(Resource)]
pub struct GameServerInfo(pub ServerInfo);

#[derive(Resource)]
pub struct NetworkClient {
    //pub player_id,
    pub peer: GamePeer,
    pub connection: Option<GameConnection>,
    pub reliable_stream: Option<GameStream>,
    pub unreliable_stream: Option<GameStream>,
}

/*
#[derive(Event, Debug)]
pub struct NetworkMessageEvent(pub GameNetworkEvent);
*/

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        let peer = GamePeer::new(UdpBackend::new());

        app.insert_resource(NetworkClient { 
                peer, 
                connection: None,
                reliable_stream: None,
                unreliable_stream : None,
            })
            //.add_event::<NetworkMessageEvent>()
            .add_systems(OnEnter(AppState::Connecting), setup_connection)
            .add_systems(Update, connection_request.run_if(in_state(AppState::Connecting)))
            .add_systems(Update, network_poll);
            //.add_systems(OnExit(AppState::InGame), disconnection_handler);
    }
}

// Initiate connection with game server
fn setup_connection(client: ResMut<NetworkClient>,
    game_server: ResMut<GameServerInfo>
) {
    if let Err(e) = client.peer.connect(&game_server.0.ip, game_server.0.port) {
        error!("Connection failed: {:?}", e);
    } else {
        println!("Connecting to game server...");
    }
}

pub fn connection_request(
    client: ResMut<NetworkClient>,
) {
    match &client.connection {
        Some(connection) => {
            match &client.reliable_stream {
                Some(stream) => {
                    let _ = client.peer.send(&connection, &stream, "JOIN Caillou".into());
                }
                None => {}
            }
        }
        None => {}
    }
}

pub fn network_poll(
    mut client: ResMut<NetworkClient>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    while let Ok(Some(event)) = client.peer.poll() {
        match event {
            GameNetworkEvent::Connected(connection) => {
                println!("Connected to server: {:?}", connection);
                client.connection = Some(connection);
                let _ = client.peer.create_stream(connection, GameStreamReliability::Reliable);
            }
            GameNetworkEvent::Disconnected(connection) => {
                println!("Disconnected from server: {:?}", connection);
                
                client.connection = None;
                client.reliable_stream = None;
                client.unreliable_stream = None;

                next_state.set(AppState::Disconnected);
            }
            GameNetworkEvent::Message { connection: _connection, stream, data } => {
                println!("MSG = {:?}", data);

                if stream.is_reliable() {
                    let msg = String::from_utf8_lossy(&data);
                    let msg = msg.trim();

                    if msg.starts_with("WELCOME ") { // Welcomed by the dedicated server -> officially connected
                        //let username = msg.replace("WELCOME ", "").trim().to_string();
                        next_state.set(AppState::InGame);
                    }
                    if msg.starts_with("REJECT ") { // Rejected by the dedicated server -> server is full, must try another server
                        next_state.set(AppState::Rejected);
                    }
                } else {
                    println!("GameplayMessgage !");
                }
            }
            GameNetworkEvent::Error { connection: _connection, inner } => {
                eprintln!("Error from server: {:?}", inner);
            }
            GameNetworkEvent::StreamCreated(_connection, stream) => {
                eprintln!("Stream created : {:?}", stream);
                if stream.is_reliable() {
                    client.reliable_stream = Some(stream);
                } else {
                    client.unreliable_stream = Some(stream);
                }
            }
            GameNetworkEvent::StreamClosed(_, _) => {
            }
        }
    }
}

/*
fn disconnection_handler(
    mut client: ResMut<NetworkClient>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    println!("Disconnecting from game server...");

    // 1. fermer proprement le peer (envoie Shutdown + join thread)
    if let Err(e) = client.peer.shutdown() {
        eprintln!("Error during shutdown: {:?}", e);
    }

    next_state.set(AppState::Disconnected);
}
*/