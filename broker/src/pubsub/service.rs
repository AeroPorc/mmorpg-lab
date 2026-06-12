use std::collections::HashMap;
use game_sockets::{GameConnection, GameStream};

use shared::messages::topics::Topic;

pub struct Service {
    pub connection: GameConnection,

    pub stream_lifeline: GameStream,

    pub publications: HashMap<GameStream, Topic>,

    pub subscriptions: HashMap<Topic, GameStream>,
}

impl Service {
    pub fn is_lifeline_stream(
        &self,
        stream: &GameStream,
    ) -> bool {
        *stream == self.stream_lifeline
    }
}

pub struct ServicesRegistry (pub HashMap<GameConnection, Service>);