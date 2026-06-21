use std::collections::{HashMap, HashSet};
use bytes::Bytes;
use game_sockets::{GamePeer, GameConnection, GameStream};

use shared::messages::{netmessage::{send_msg, PubSubMessage, PubSubOp}, topics::Topic};
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
    pub clients: HashMap<u32, GameConnection>,
}

impl Broker {
    pub fn new(peer: GamePeer) -> Self {
        Self {
            peer,
            services: ServicesRegistry(HashMap::new()),
            topics: TopicsRegistry(HashMap::new()),
            clients: HashMap::new(),
        }
    }

    pub fn register_client_id(&mut self, client_id: u32, connection: GameConnection) {
        self.clients.insert(client_id, connection);
    }

    pub fn force_subscribe_client(&mut self, client_id: u32, topic: Topic) {
        let Some(&connection) = self.clients.get(&client_id) else {
            return;
        };
        let stream = self
            .services
            .0
            .get(&connection)
            .and_then(|s| s.subscriptions.values().next().cloned());
        let Some(stream) = stream else {
            return;
        };
        self.subscribe(topic.clone(), connection, stream);
        println!("Broker: forced client {} to subscribe {:?}", client_id, topic);
    }

    pub fn force_unsubscribe_client(&mut self, client_id: u32, topic: Topic) {
        let Some(&connection) = self.clients.get(&client_id) else {
            return;
        };
        let removed = self
            .services
            .0
            .get_mut(&connection)
            .and_then(|s| s.subscriptions.remove(&topic));
        if let Some(stream) = removed {
            if let Some(subs) = self.topics.0.get_mut(&topic) {
                subs.remove(&Subscriber { connection, stream });
            }
            println!("Broker: forced client {} to unsubscribe {:?}", client_id, topic);
        }
    }

    pub fn is_existing_service(&mut self, connection: &GameConnection) -> bool {
        self.services.0.contains_key(connection)
    }

    pub fn is_existing_lifeline(
        &mut self,
        connection: &GameConnection,
        stream_reliable: &GameStream,
    ) -> bool {
        if let Some(service) = self.services.0.get(&connection) {
            return service.is_lifeline_stream(&stream_reliable);
        }
        false
    }

    pub fn register_service(&mut self, connection: &GameConnection, stream_reliable: &GameStream) {
        self.services.0.insert(
            *connection,
            Service {
                connection: *connection,
                stream_lifeline: stream_reliable.clone(),
                publications: HashMap::new(),
                subscriptions: HashMap::new(),
            },
        );
    }

    pub fn remove_service(&mut self, connection: &GameConnection) {
        let (subscriptions, publications) = match self.services.0.get(&connection) {
            Some(service) => {
                let subscriptions: Vec<Topic> = service.subscriptions.keys().cloned().collect();
                let publications: Vec<Topic> = service.publications.values().cloned().collect();
                (subscriptions, publications)
            }
            None => return,
        };

        for topic in subscriptions {
            self.unsubscribe(topic, *connection);
        }
        for topic in publications {
            self.suppress_topic(topic, *connection);
        }
    }

    pub fn create_topic(&mut self, topic: Topic, connection: GameConnection, stream: GameStream) {
        self.topics.0.entry(topic.clone()).or_insert_with(HashSet::new);

        if let Some(service) = self.services.0.get_mut(&connection) {
            service.publications.insert(stream, topic);
        }
    }

    pub fn forced_create_topic(&mut self, topic: &Topic, connection: &GameConnection) {
        let Some(service) = self.services.0.get(&connection) else {
            return;
        };

        self.topics.0.entry(topic.clone()).or_insert_with(HashSet::new);

        let forced_pub_msg = PubSubMessage {
            op: PubSubOp::ForcedPub,
            topic: topic.clone(),
            stream: None,
            target: None,
        };

        let _ = send_msg(&self.peer, &connection, &service.stream_lifeline, &forced_pub_msg);
    }

    pub fn suppress_topic(&mut self, topic: Topic, connection: GameConnection) {
        let subscribers = match self.topics.0.get(&topic) {
            Some(s) => s,
            None => return,
        };

        let subs: Vec<_> = subscribers.into_iter().cloned().collect();
        for subscriber in subs {
            self.unsubscribe(topic.clone(), subscriber.connection)
        }

        if let Some(service) = self.services.0.get_mut(&connection) {
            let stream = service.publications.iter().find_map(|(stream, t)| {
                if *t == topic { Some(stream.clone()) } else { None }
            });

            if let Some(stream) = stream {
                service.publications.remove(&stream);
            }
        }

        self.topics.0.remove(&topic);

        if let Some(service) = self.services.0.get(&connection) {
            let stop_pub_msg = PubSubMessage {
                op: PubSubOp::StopPub,
                topic: topic.clone(),
                stream: None,
                target: None,
            };

            let _ = send_msg(&self.peer, &connection, &service.stream_lifeline, &stop_pub_msg);
        }
    }

    pub fn subscribe(&mut self, topic: Topic, connection: GameConnection, stream: GameStream) {
        let subscriber = Subscriber {
            connection,
            stream: stream.clone(),
        };

        self.topics
            .0
            .entry(topic.clone())
            .or_insert_with(HashSet::new)
            .insert(subscriber);

        if let Some(service) = self.services.0.get_mut(&connection) {
            service.subscriptions.insert(topic, stream.clone());
        }
    }

    pub fn forced_subscribe(&mut self, topic: &Topic, connection: &GameConnection) {
        if let Some(service) = self.services.0.get(&connection) {
            let forced_sub_msg = PubSubMessage {
                op: PubSubOp::ForcedSub,
                topic: topic.clone(),
                stream: None,
                target: None,
            };

            let _ = send_msg(&self.peer, &connection, &service.stream_lifeline, &forced_sub_msg);
        }
    }

    pub fn unsubscribe(&mut self, topic: Topic, connection: GameConnection) {
        let Some(service) = self.services.0.get_mut(&connection) else {
            return;
        };

        let Some(stream) = service.subscriptions.remove(&topic) else {
            return;
        };

        if let Some(subscribers) = self.topics.0.get_mut(&topic) {
            subscribers.remove(&Subscriber { connection, stream });
        }
    }

    pub fn publish(&mut self, connection: &GameConnection, stream: &GameStream, data: Bytes) {
        let Some(service) = self.services.0.get(&connection) else {
            return;
        };

        let Some(topic) = service.publications.get(&stream) else {
            return;
        };

        let Some(subscribers) = self.topics.0.get(&topic) else {
            return;
        };

        for subscriber in subscribers.iter() {
            let _ = self.peer.send(&subscriber.connection, &subscriber.stream, data.clone());
        }
    }
}
