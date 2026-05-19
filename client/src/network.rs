use shared::*;
use bevy::prelude::*;
//use bevy::ecs::event::Event;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GamePeer, GameNetworkEvent, /*GameConnection, GameStream,*/ GameStreamReliability};

//use bytes::Bytes;
//use uuid::Uuid;

use crate::AppState;

#[derive(Resource)]
pub struct GameServerInfo(pub ServerInfo);

#[derive(Resource)]
pub struct NetworkClient {
    //pub player_id,
    pub peer: GamePeer,
}

/*
#[derive(Event, Debug)]
pub struct NetworkIncomingEvent(pub GameNetworkEvent);

#[derive(Event, Debug)]
pub struct NetworkSendEvent {
    pub connection: GameConnection,
    pub stream: GameStream,
    pub data: Bytes,
}
*/

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        let peer = GamePeer::new(UdpBackend::new());

        app.insert_resource(NetworkClient { peer })
            //.add_event::<NetworkIncomingEvent>()
            //.add_event::<NetworkSendEvent>()
            .add_systems(OnEnter(AppState::Connecting), setup_connection)
            .add_systems(Update, (
                network_poll,
            ).chain());
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

pub fn network_poll(
    mut client: ResMut<NetworkClient>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    while let Ok(Some(event)) = client.peer.poll() {
        match event {
            GameNetworkEvent::Connected(connection) => {
                println!("Connected to server: {:?}", connection);
                let _ = client.peer.create_stream(connection, GameStreamReliability::Unreliable);
            }
            GameNetworkEvent::Disconnected(connection) => {
                println!("Disconnected from server: {:?}", connection);
            }
            GameNetworkEvent::Message { connection: _, stream: _, data } => {
                println!("MSG = {:?}", data);
            }
            GameNetworkEvent::Error { connection: _connection, inner } => {
                eprintln!("Error from server: {:?}", inner);
            }
            GameNetworkEvent::StreamCreated(connection, stream) => {
                eprintln!("Stream created : {:?}", stream);
                let _ = client.peer.send(&connection, &stream, "JOIN Pierre".into());
                next_state.set(AppState::InGame);
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