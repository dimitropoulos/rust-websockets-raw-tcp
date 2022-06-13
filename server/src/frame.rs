// use crate::error::Result;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::{
    io::{ErrorKind, Read, Write},
    result::Result,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Data {
    /// 0x0 denotes a continuation frame
    Continue,
    /// 0x1 denotes a text frame
    Text,
    /// 0x2 denotes a binary frame
    Binary,
    /// 0x3-7 are reserved for further non-control frames
    Reserved(u8),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Control {
    /// 0x8 denotes a connection close
    Close,
    /// 0x9 denotes a ping
    Ping,
    /// 0xa denotes a pong
    Pong,
    /// 0xb-f are reserved for further control frames
    Reserved(u8),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum OpCode {
    /// Data (text or binary).
    Data(Data),
    /// Control message (close, ping, pong).
    Control(Control),
}

impl From<OpCode> for u8 {
    fn from(opcode: OpCode) -> Self {
        use self::{
            Control::{Close, Ping, Pong, Reserved as ControlReserved},
            Data::{Binary, Continue, Reserved as DataReserved, Text},
            OpCode::{Control, Data},
        };
        match opcode {
            Data(Continue) => 0,
            Data(Text) => 1,
            Data(Binary) => 2,
            Data(DataReserved(i)) => i,
            Control(Close) => 8,
            Control(Ping) => 9,
            Control(Pong) => 10,
            Control(ControlReserved(i)) => i,
        }
    }
}

impl From<u8> for OpCode {
    fn from(byte: u8) -> OpCode {
        use self::{
            Control::{Close, Ping, Pong, Reserved as ControlReserved},
            Data::{Binary, Continue, Reserved as DataReserved, Text},
            OpCode::{Control, Data},
        };
        match byte {
            0 => Data(Continue),
            1 => Data(Text),
            2 => Data(Binary),
            i @ 3..=7 => Data(DataReserved(i)),
            8 => Control(Close),
            9 => Control(Ping),
            10 => Control(Pong),
            i @ 11..=15 => Control(ControlReserved(i)),
            _ => panic!("invalid opcode {}", byte),
        }
    }
}

/// A struct representing a WebSocket frame header
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FrameHeader {
    /// Indicates that the frame is the last one of a possibly fragmented message.
    pub is_final: bool,
    /// Reserved for protocol extensions.
    pub rsv1: bool,
    /// Reserved for protocol extensions.
    pub rsv2: bool,
    /// Reserved for protocol extensions.
    pub rsv3: bool,
    /// WebSocket protocol opcode.
    pub opcode: OpCode,
    /// A frame mask, if any.
    pub mask: Option<[u8; 4]>,
}

/// Handling of the length format.
enum LengthFormat {
    U8(u8),
    U16,
    U64,
}

impl LengthFormat {
    /// Get the length format for a given data size
    fn for_length(length: u64) -> Self {
        if length < 126 {
            return LengthFormat::U8(length as u8);
        }
        if length < 65536 {
            return LengthFormat::U16;
        }
        LengthFormat::U64
    }

    /// Encode the length according to the RFC
    fn length_byte(&self) -> u8 {
        match *self {
            LengthFormat::U8(b) => b,
            LengthFormat::U16 => 126,
            LengthFormat::U64 => 127,
        }
    }

    fn extra_bytes(&self) -> usize {
        match *self {
            LengthFormat::U8(_) => 0,
            LengthFormat::U16 => 2,
            LengthFormat::U64 => 8,
        }
    }

    fn for_byte(byte: u8) -> Self {
        match byte & 0b0111_1111 {
            126 => LengthFormat::U16,
            127 => LengthFormat::U64,
            b => LengthFormat::U8(b),
        }
    }
}

impl FrameHeader {
    pub(crate) fn set_random_mask(&mut self) {
        self.mask = Some(rand::random())
    }

    pub fn parse(input: &mut impl Read) -> Result<Option<(Self, u64)>, Box<dyn std::error::Error>> {
        let mut head = [0u8; 2];
        if input.read(&mut head)? != 2 {
            return Ok(None);
        }
        let first = head[0];
        let second = head[1];

        let is_final = first & 0b1000_0000 != 0;

        let rsv1 = first & 0b0100_0000 != 0;
        let rsv2 = first & 0b0010_0000 != 0;
        let rsv3 = first & 0b0001_0000 != 0;

        let opcode = OpCode::from(first & 0b0000_1111);
        let masked = second & 0b1000_0000 != 0;

        let length = {
            let length_byte = second & 0b0111_1111;
            let length_length = LengthFormat::for_byte(length_byte).extra_bytes();
            if length_length > 0 {
                match input.read_uint::<NetworkEndian>(length_length) {
                    Err(ref err) if err.kind() == ErrorKind::UnexpectedEof => {
                        return Ok(None);
                    }
                    Err(err) => {
                        return Err(err.into());
                    }
                    Ok(read) => read,
                }
            } else {
                u64::from(length_byte)
            }
        };

        let mask = if masked {
            let mut mask_bytes = [0u8; 4];
            if input.read(&mut mask_bytes)? != 4 {
                return Ok(None);
            } else {
                Some(mask_bytes)
            }
        } else {
            None
        };

        let header = FrameHeader {
            is_final,
            rsv1,
            rsv2,
            rsv3,
            opcode,
            mask,
        };
        Ok(Some((header, length)))
    }

    pub fn format(
        &self,
        length: u64,
        output: &mut impl Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let code: u8 = self.opcode.into();
        let one = code | if self.is_final { 0x80 } else { 0 };

        let length_format = LengthFormat::for_length(length);

        let two = length_format.length_byte() | if self.mask.is_some() { 0x80 } else { 0 };

        output.write_all(&[one, two])?;

        match length_format {
            LengthFormat::U8(_) => (),
            LengthFormat::U16 => output.write_u16::<NetworkEndian>(length as u16)?,
            LengthFormat::U64 => output.write_u64::<NetworkEndian>(length)?,
        }

        if let Some(ref mask) = self.mask {
            output.write_all(mask)?
        }
        Ok(())
    }

    pub fn len(&self, length: u64) -> usize {
        2 + LengthFormat::for_length(length).extra_bytes() + if self.mask.is_some() { 4 } else { 0 }
    }
}

pub fn apply_mask(buf: &mut [u8], mask: [u8; 4]) {
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte ^= mask[i & 3];
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Frame {
    header: FrameHeader,
    payload: Vec<u8>,
}

impl Frame {
    pub fn message(payload: Vec<u8>, opcode: OpCode) -> Frame {
        Frame {
            header: FrameHeader {
                is_final: true,
                opcode,
                rsv1: false,
                rsv2: false,
                rsv3: false,
                mask: None,
            },
            payload,
        }
    }

    pub(crate) fn apply_mask(&mut self) {
        if let Some(mask) = self.header.mask.take() {
            apply_mask(&mut self.payload, mask)
        }
    }

    pub fn format(mut self, output: &mut impl Write) -> Result<(), Box<dyn std::error::Error>> {
        self.header.format(self.payload.len() as u64, output)?;
        self.apply_mask();
        output.write_all(&self.payload)?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        let payload_length = self.payload.len();
        let header_length = self.header.len(payload_length as u64);
        header_length + payload_length
    }
}
