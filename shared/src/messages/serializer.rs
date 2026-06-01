use bytes::{Buf, BufMut, Bytes, BytesMut};

pub struct Serializer {
    buffer: BytesMut,
}

impl Serializer {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(256),
        }
    }

    pub fn into_bytes(self) -> Bytes {
        self.buffer.freeze()
    }

    pub fn write_u8(&mut self, value: u8) {
        self.buffer.put_u8(value);
    }

    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(value as u8);
    }

    pub fn write_u16(&mut self, value: u16) {
        self.buffer.put_u16_le(value);
    }

    pub fn write_u32(&mut self, value: u32) {
        self.buffer.put_u32_le(value);
    }

    pub fn write_i32(&mut self, value: i32) {
        self.buffer.put_i32_le(value);
    }

    pub fn write_f32(&mut self, value: f32) {
        self.buffer.put_f32_le(value);
    }

    pub fn write_string(&mut self, value: &str) {
        let bytes = value.as_bytes();

        self.write_u16(bytes.len() as u16);

        self.buffer.extend_from_slice(bytes);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }
}

pub struct Deserializer {
    buffer: Bytes,
}

impl Deserializer {
    pub fn new(buffer: Bytes) -> Self {
        Self { buffer }
    }

    pub fn read_u8(&mut self) -> u8 {
        self.buffer.get_u8()
    }

    pub fn read_bool(&mut self) -> bool {
        self.read_u8() != 0
    }

    pub fn read_u16(&mut self) -> u16 {
        self.buffer.get_u16_le()
    }

    pub fn read_u32(&mut self) -> u32 {
        self.buffer.get_u32_le()
    }

    pub fn read_i32(&mut self) -> i32 {
        self.buffer.get_i32_le()
    }

    pub fn read_f32(&mut self) -> f32 {
        self.buffer.get_f32_le()
    }

    pub fn read_string(&mut self) -> String {
        let len = self.read_u16() as usize;

        let bytes = self.buffer.copy_to_bytes(len);

        String::from_utf8(bytes.to_vec()).unwrap()
    }

    pub fn remaining(&self) -> usize {
        self.buffer.remaining()
    }
}