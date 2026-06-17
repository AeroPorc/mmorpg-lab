use bevy::prelude::*;
use bytes::Bytes;

use crate::AppState;
use crate::network::{LocalPlayer, NetworkClient};

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, send_input.run_if(in_state(AppState::InGame)));
    }
}

fn axis_to_byte(axis: f32) -> u8 {
    (((axis + 1.0) * 0.5) * 255.0).round().clamp(0.0, 255.0) as u8
}

fn send_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    client: Res<NetworkClient>,
    local: Res<LocalPlayer>,
) {
    let (Some(connection), Some(stream)) = (&client.connection, &client.unreliable_stream) else {
        return;
    };

    let right = keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight);
    let left = keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft);
    let up = keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp);
    let down = keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown);

    let x = right as i32 as f32 - left as i32 as f32;
    let y = up as i32 as f32 - down as i32 as f32;

    let mut payload = Vec::with_capacity(1 + 4 + 16);
    payload.push(0x05);
    payload.extend_from_slice(&local.id.to_le_bytes());
    let mut input = [127u8; 16];
    input[0] = axis_to_byte(x);
    input[1] = axis_to_byte(y);
    payload.extend_from_slice(&input);

    let _ = client.peer.send(connection, stream, Bytes::from(payload));
}
