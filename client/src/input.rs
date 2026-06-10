use bevy::prelude::*;

use shared::messages::netmessage::{send_msg, Input, InputMessage};

use crate::AppState;
use crate::NetworkClient;

#[derive(Resource, Default)]
pub struct InputMessageResource(pub InputMessage);

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InputMessageResource>()
            .add_systems(Update, (handle_inputs).run_if(in_state(AppState::InGame)));
    }
}

fn handle_inputs(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut input_msg: ResMut<InputMessageResource>,
    client: ResMut<NetworkClient>,
) {
    let input = Input {
        up: keyboard.pressed(KeyCode::KeyW)
            || keyboard.pressed(KeyCode::ArrowUp),

        down: keyboard.pressed(KeyCode::KeyS)
            || keyboard.pressed(KeyCode::ArrowDown),

        left: keyboard.pressed(KeyCode::KeyA)
            || keyboard.pressed(KeyCode::ArrowLeft),

        right: keyboard.pressed(KeyCode::KeyD)
            || keyboard.pressed(KeyCode::ArrowRight),
    };

    input_msg.0.push(input);

    let Some(connection) = client.connection.as_ref() else {
        return;
    };

    let Some(stream) = client.unreliable_stream.as_ref() else {
        return;
    };

    // Désormais factorisé dans la fonction send_msg de la shared library
    //let mut serializer = Serializer::new();
    //input_msg.0.serialize(&mut serializer);
    //let _ = client.peer.send(connection, stream, serializer.buffer.into());

    let _ = send_msg(&client.peer, &connection, &stream, &input_msg.0);
}