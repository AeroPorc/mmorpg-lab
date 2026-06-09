use std::collections::HashMap;
use game_sockets::{GameConnection, GameStream};

use super::topics::Topic;

pub struct Service {
    pub connection: GameConnection,

    //pub stream_reliable: GameStream,

    pub publications: HashMap<GameStream, Topic>,

    pub subscriptions: HashMap<Topic, GameStream>,
}

pub struct ServicesRegistry (pub HashMap<GameConnection, Service>);