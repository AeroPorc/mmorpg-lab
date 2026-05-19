use shared::*;
use bevy::prelude::*;
use game_sockets::protocols::UdpBackend;
use game_sockets::{GamePeer, GameNetworkEvent, GameConnection, GameStream};
//use bytes::Bytes;

use crate::AppState;

pub struct NetworkPlugin;

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

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        let peer = GamePeer::new(UdpBackend::new());

        app.insert_resource(NetworkClient { peer })
            //.add_event::<NetworkIncomingEvent>()
            //.add_event::<NetworkSendEvent>()
            .add_systems(OnEnter(AppState::Connecting), (
                bind_socket,
                setup_connection,
            ).chain());
            /*
            .add_systems(Update, (
                network_poll_system,
                network_send_system,
            ));
            */
    }
}

fn bind_socket(client: ResMut<NetworkClient>) {
    if let Err(e) = client.peer.listen("127.0.0.1", 5001) {
        error!("Failed to bind socket: {:?}", e);
    } else {
        println!("Client listening on 0.0.0.0:5000");
    }
}

fn setup_connection(client: ResMut<NetworkClient>,
    game_server: ResMut<GameServerInfo>
) {
    if let Err(e) = client.peer.connect(&game_server.0.ip, game_server.0.port) {
        error!("Connection failed: {:?}", e);
    } else {
        println!("Connecting to game server...");
    }
    //client.peer.send(, , "lol");
}