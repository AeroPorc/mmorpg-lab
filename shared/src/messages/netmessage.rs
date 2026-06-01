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