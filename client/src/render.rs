use bevy::prelude::*;
use std::collections::HashMap;

use crate::network::{LocalPlayer, PlayerStats, WorldView};

pub struct RenderPlugin;

#[derive(Component)]
struct GameCamera;

#[derive(Component)]
struct HudText;

#[derive(Resource, Default)]
struct SpriteIndex {
    players: HashMap<u32, Entity>,
    enemies: HashMap<u32, Entity>,
    projectiles: HashMap<u32, Entity>,
}

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteIndex>()
            .add_systems(Startup, setup)
            .add_systems(Update, (sync_world, follow_camera, update_hud));
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, Transform::from_xyz(0.0, 0.0, 1000.0), GameCamera));

    commands.spawn((
        Text::new("HP: --\nScore: 0\nWave: 1"),
        TextFont { font_size: 22.0, ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        HudText,
    ));
}

fn follow_camera(
    local: Res<LocalPlayer>,
    world: Res<WorldView>,
    mut cam: Query<&mut Transform, With<GameCamera>>,
) {
    if let Some(&pos) = world.players.get(&local.id) {
        if let Some(mut t) = cam.iter_mut().next() {
            t.translation.x = pos.x;
            t.translation.y = pos.y;
        }
    }
}

fn update_hud(stats: Res<PlayerStats>, mut query: Query<&mut Text, With<HudText>>) {
    for mut text in &mut query {
        text.0 = format!(
            "HP: {}\nScore: {}\nWave: {}",
            stats.hp.max(0),
            stats.score,
            stats.wave.max(1)
        );
    }
}

fn sync_world(
    mut commands: Commands,
    world: Res<WorldView>,
    local: Res<LocalPlayer>,
    mut index: ResMut<SpriteIndex>,
    mut transforms: Query<&mut Transform, Without<GameCamera>>,
) {
    // --- Players ---
    for (&id, &pos) in world.players.iter() {
        let z = if id == local.id { 2.0 } else { 1.0 };
        if let Some(&entity) = index.players.get(&id) {
            if let Ok(mut t) = transforms.get_mut(entity) {
                t.translation = Vec3::new(pos.x, pos.y, z);
            }
        } else {
            let color = if id == local.id {
                Color::srgb(0.2, 1.0, 0.3) 
            } else {
                Color::srgb(0.3, 0.6, 1.0) 
            };
            let entity = commands
                .spawn((
                    Sprite::from_color(color, Vec2::splat(12.0)),
                    Transform::from_xyz(pos.x, pos.y, z),
                ))
                .id();
            index.players.insert(id, entity);
        }
    }
    index.players.retain(|id, entity| {
        if world.players.contains_key(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    for (&id, &pos) in world.enemies.iter() {
        if let Some(&entity) = index.enemies.get(&id) {
            if let Ok(mut t) = transforms.get_mut(entity) {
                t.translation = Vec3::new(pos.x, pos.y, 0.0);
            }
        } else {
            let entity = commands
                .spawn((
                    Sprite::from_color(Color::srgb(1.0, 0.3, 0.25), Vec2::splat(7.0)),
                    Transform::from_xyz(pos.x, pos.y, 0.0),
                ))
                .id();
            index.enemies.insert(id, entity);
        }
    }
    index.enemies.retain(|id, entity| {
        if world.enemies.contains_key(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });

    // --- Projectiles ---
    for (&id, &pos) in world.projectiles.iter() {
        if let Some(&entity) = index.projectiles.get(&id) {
            if let Ok(mut t) = transforms.get_mut(entity) {
                t.translation = Vec3::new(pos.x, pos.y, 3.0);
            }
        } else {
            let entity = commands
                .spawn((
                    Sprite::from_color(Color::srgb(1.0, 0.95, 0.4), Vec2::splat(4.0)),
                    Transform::from_xyz(pos.x, pos.y, 3.0),
                ))
                .id();
            index.projectiles.insert(id, entity);
        }
    }
    index.projectiles.retain(|id, entity| {
        if world.projectiles.contains_key(id) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}
