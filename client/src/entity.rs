use bevy::prelude::*;

use crate::AppState;

#[derive(Component, Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub struct NetworkId(pub u32);

#[derive(Bundle)]
pub struct EntityBundle {
    pub id: NetworkId,
    pub transform: Transform,

    pub mesh: Mesh3d,
    pub material: MeshMaterial3d<StandardMaterial>,
}

pub fn spawn_entity(
    commands: &mut Commands,
    id: u32,
    pos: Vec3,
) {
    commands.spawn(EntityBundle {
        id: NetworkId(id),
        transform: Transform::from_translation(pos),

        mesh: Mesh3d(default()),
        material: MeshMaterial3d(default::<StandardMaterial>()),
    });
}