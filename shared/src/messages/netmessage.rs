use super::serializer::{Serializer, Deserializer};
use super::topics::Topic;

use bytes::{Bytes};
use game_sockets::{GamePeer, GameConnection, GameStream, GameSocketError};

pub fn send_msg<M: NetMessage>(
    peer: &GamePeer,
    connection: &GameConnection,
    stream: &GameStream,
    msg: &M,
) -> Result<(), GameSocketError> {
    let mut serializer = Serializer::new();

    serializer.write_u8(M::ID as u8);

    msg.serialize(&mut serializer);

    let bytes = serializer.into_bytes();

    peer.send(connection, stream, bytes)
}

pub fn decode_msg(bytes: &Bytes) -> Option<AnyMessage> {
    let mut deserializer = Deserializer::new(bytes.clone());

    let id = MessageId::from_u8(deserializer.read_u8())?;

    Some(match id {
        MessageId::Input => {
            AnyMessage::Input(InputMessage::deserialize(&mut deserializer))
        }
        MessageId::Snapshot => {
            AnyMessage::Snapshot(SnapshotMessage::deserialize(&mut deserializer))
        }
        MessageId::PubSub => {
            AnyMessage::PubSub(PubSubMessage::deserialize(&mut deserializer))
        }
        _ => {
            return None;
        }
    })
}

#[repr(u8)]
pub enum MessageId {
    Chat = 0,
    Input = 1,
    Snapshot = 2,
    PubSub = 3,
}

impl MessageId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Chat),
            1 => Some(Self::Input),
            2 => Some(Self::Snapshot),
            3 => Some(Self::PubSub),
            _ => None,
        }
    }
}

pub enum AnyMessage {
    Chat(),
    Input(InputMessage),
    Snapshot(SnapshotMessage),
    PubSub(PubSubMessage),
}

pub trait NetMessage: Sized {
    const ID: MessageId;

    fn serialize(&self, serializer: &mut Serializer);

    fn deserialize(deserializer: &mut Deserializer) -> Self;
}

#[derive(Clone, Copy, Default)]
pub struct Input {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl Input {
    fn serialize(&self, serializer: &mut Serializer) {
        let mut bits = 0u8;

        bits |= (self.up as u8) << 0;
        bits |= (self.down as u8) << 1;
        bits |= (self.left as u8) << 2;
        bits |= (self.right as u8) << 3;

        serializer.write_u8(bits);
    }

    fn deserialize(deserializer: &mut Deserializer) -> Self {
        let bits = deserializer.read_u8();

        Self {
            up: (bits & (1 << 0)) != 0,
            down: (bits & (1 << 1)) != 0,
            left: (bits & (1 << 2)) != 0,
            right: (bits & (1 << 3)) != 0,
        }
    }
}

pub struct InputMessage {
    pub inputs: [Input; 20],
    pub len: usize,
    pub latest: u32,
}

impl InputMessage {
    pub fn new() -> Self {
        Self {
            inputs: [Input::default(); 20],
            len: 0,
            latest: 0, // id of the latest input
        }
    }

    pub fn push(&mut self, input: Input) {
        self.latest += 1;

        if self.len < self.inputs.len() {
            self.inputs[self.len] = input;
            self.len += 1;
        } else {
            // Décale tout à gauche
            self.inputs.rotate_left(1);

            // Remplace le plus récent
            self.inputs[self.inputs.len() - 1] = input;
        }
    }

    pub fn latest(&self) -> Option<&Input> {
        if self.len == 0 {
            None
        } else {
            Some(&self.inputs[self.len - 1])
        }
    }

    pub fn oldest_id(&self) -> Option<u32> {
        if self.len == 0 {
            None
        } else {
            Some(self.latest - self.len as u32 + 1)
        }
    }

    pub fn get_by_id(&self, id: u32) -> Option<&Input> {
        if self.len == 0 {
            return None;
        }

        let oldest_id = self.oldest_id()?;

        if id < oldest_id || id > self.latest {
            return None;
        }

        let index = (id - oldest_id) as usize;
        Some(&self.inputs[index])
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for InputMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl NetMessage for InputMessage {
    const ID: MessageId = MessageId::Input;

    fn serialize(&self, serializer: &mut Serializer) {
        serializer.write_u8(self.len as u8);
        serializer.write_u32(self.latest);

        for input in &self.inputs {
            input.serialize(serializer);
        }
    }

    fn deserialize(deserializer: &mut Deserializer) -> Self {
        let len = deserializer.read_u8() as usize;
        let latest = deserializer.read_u32();
        
        let mut inputs = [Input::default(); 20];

        for input in &mut inputs {
            *input = Input::deserialize(deserializer);
        }

        Self { inputs, len, latest }
    }
}

pub struct SnapshotMessage {
    pub tmp_val: u8,
}

impl NetMessage for SnapshotMessage {
    const ID: MessageId = MessageId::Snapshot;

    fn serialize(&self, serializer: &mut Serializer) {
        serializer.write_u8(self.tmp_val);
    }

    fn deserialize(deserializer: &mut Deserializer) -> Self {
        Self {
            tmp_val: deserializer.read_u8(),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum PubSubOp {
    Pub = 0, // Sent by the publisher to the broker to registrer as the publisher of a new topic
    ForcedPub = 1, // Sent by the broker to force registration as a publisher on a new topic
    StopPub = 2, // Initiate the stop of the publication
    Sub = 3, // Sent by the subscriber to the broker to registrer a new subscriber of a topic
    ForcedSub = 4, // Sent by the broker to force registration as a subscriber on a topic
    StopSub = 5, // Initiate the stop of the subscription
    Helo = 6,
    End = 7, // Initiate the termination of all subscriptions and publications related to the sender
    Err = 8,
}

pub struct PubSubMessage {
    pub op: PubSubOp,
    pub topic: Topic,
    pub stream: Option<GameStream>,
    pub target: Option<u32>,
}

impl NetMessage for PubSubMessage {
    const ID: MessageId = MessageId::PubSub;

    fn serialize(&self, serializer: &mut Serializer) {
        // 1. op
        serializer.write_u8(self.op as u8);

        // 2. topic
        serializer.write_topic(&self.topic);

        // 3. stream 
        match &self.stream {
            Some(stream) => {
                serializer.write_bool(true);
                serializer.write_u16(stream.stream_id);
            }
            None => {
                serializer.write_bool(false);
            }
        }
        match &self.target {
            Some(target) => {
                serializer.write_bool(true);
                serializer.write_u32(*target);
            }
            None => {
                serializer.write_bool(false);
            }
        }
    }

    fn deserialize(deserializer: &mut Deserializer) -> Self {
        let op_u8 = deserializer.read_u8();

        let op = match op_u8 {
            0 => PubSubOp::Pub,
            1 => PubSubOp::ForcedPub,
            2 => PubSubOp::StopPub,
            3 => PubSubOp::Sub,
            4 => PubSubOp::ForcedSub,
            5 => PubSubOp::StopSub,
            6 => PubSubOp::Helo,
            7 => PubSubOp::End,
            _ => PubSubOp::Err,
        };

        let topic = deserializer.read_topic();
        
        let stream = match deserializer.read_bool() {
            true => Some(GameStream {
                stream_id: deserializer.read_u16(),
            }),
            false => None,
        };

        let target = match deserializer.read_bool() {
            true => Some(deserializer.read_u32()),
            false => None,
        };

        Self { op, topic, stream, target }
    }
}