use std::collections::{HashMap, HashSet};
use bytes::Bytes;
use game_sockets::{GamePeer, GameConnection, GameStream};

use super::topics::Topic;
use super::service::*;

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct Subscriber {
    pub connection: GameConnection,
    pub stream: GameStream,
}

pub struct TopicsRegistry (pub HashMap<Topic, HashSet<Subscriber>>);

pub struct Broker {
    pub peer: GamePeer,

    pub services: ServicesRegistry,
    pub topics: TopicsRegistry,
}

impl Broker {
    pub fn new(peer: GamePeer) -> Self {
        Self {
            peer,
            services: ServicesRegistry(HashMap::new()),
            topics: TopicsRegistry(HashMap::new()),
        }
    }

    pub fn register_service(
        &mut self,
        connection: GameConnection,
    ) {
        self.services.0.insert(
            connection,
            Service {
                connection,
                publications: HashMap::new(),
                subscriptions: HashMap::new(),
            },
        );
    }

    pub fn remove_service(
        &mut self,
        connection: GameConnection,
    ) {
    }

    pub fn create_topic( // create a new topic delivered by given connection as service
        &mut self,
        topic: Topic,
        connection: GameConnection,
    ) {
        self.topics
            .0
            .entry(topic.clone())
            .or_insert_with(HashSet::new);

            /*
        if let Some(service) = self.services.0.get_mut(&connection) {
            service.publications.insert(stream, topic);
        }*/
    }

    pub fn suppress_topic( // suppress a topic delivered by given connection as service
        &mut self,
        topic: Topic,
        connection: GameConnection,
    ) {
    }

    pub fn subscribe( // subscribe given subscriber to a preexisting topic
        &mut self,
        topic: Topic,
        connection: GameConnection,
    ) {
    }

    pub fn unsubscribe( // unsubscribe given subscriber to a presubscribed topic
        &mut self,
        topic: Topic,
        connection: GameConnection,
    ) {
    }

    pub fn publish( // publish the newly received data to all subscribers of the topic corresponding to (connection, stream)
        &mut self,
        connection: GameConnection,
        stream: GameStream,
        data: Bytes,
    ) {
    }
}