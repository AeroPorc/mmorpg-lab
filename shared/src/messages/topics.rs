#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum Topic {
    Input(u32),      // player_id
    Snapshot(u32),   // shard_id
    View(u32),       // shard_id ? player_id ? (POV spatial)
}