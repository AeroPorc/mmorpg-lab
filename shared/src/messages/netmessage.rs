use super::serializer::{Serializer, Deserializer};

#[repr(u8)]
pub enum MessageId {
    Chat = 0,
    Input = 1,
    Snapshot = 2,
}

impl MessageId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Chat),
            1 => Some(Self::Input),
            2 => Some(Self::Snapshot),
            _ => None,
        }
    }
}

pub trait NetMessage: Sized {
    const ID: MessageId;

    fn serialize(&self, serializer: &mut Serializer);

    fn deserialize(deserializer: &mut Deserializer) -> Self;
}

pub struct InputMessage {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl NetMessage for InputMessage {
    const ID: MessageId = MessageId::Input;

    fn serialize(&self, serializer: &mut Serializer) {
        serializer.write_bool(self.up);
        serializer.write_bool(self.down);
        serializer.write_bool(self.left);
        serializer.write_bool(self.right);
    }

    fn deserialize(deserializer: &mut Deserializer) -> Self {
        Self {
            up: deserializer.read_bool(),
            down: deserializer.read_bool(),
            left: deserializer.read_bool(),
            right: deserializer.read_bool(),
        }
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