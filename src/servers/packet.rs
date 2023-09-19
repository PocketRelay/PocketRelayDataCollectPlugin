use bitflags::bitflags;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::fmt::Debug;
use std::io;
use tdf::{
    serialize_vec, DecodeResult, TdfDeserialize, TdfDeserializer, TdfSerialize, TdfStringifier,
};
use tokio_util::codec::{Decoder, Encoder};

use crate::servers::components::{component_key, get_command_name, get_component_name};

/// The different types of packets
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    /// ID counted request packets (0x00)
    Request = 0x0,
    /// Packets responding to requests (0x10)
    Response = 0x1,
    /// Unique packets coming from the server (0x20)
    Notify = 0x2,
    /// Error packets (0x30)
    Error = 0x3,
}
bitflags! {
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    pub struct PacketOptions: u8 {
        const NONE = 0x0;
        const JUMBO_FRAME = 0x1;
        const HAS_CONTXT = 0x2;
        const IMMEDIATE = 0x4;
        const JUMBO_CONTEXT = 0x8;
    }
}

/// From u8 implementation to convert bytes back into
/// PacketTypes
impl From<u8> for PacketType {
    fn from(value: u8) -> Self {
        match value {
            0x0 => PacketType::Request,
            0x1 => PacketType::Response,
            0x2 => PacketType::Notify,
            0x3 => PacketType::Error,
            _ => PacketType::Request,
        }
    }
}

/// Structure of packet header which comes before the
/// packet content and describes it.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PacketHeader {
    pub component: u16,
    pub command: u16,
    pub error: u16,
    pub ty: PacketType,
    pub options: PacketOptions,
    pub seq: u16,
}

impl PacketHeader {
    const MIN_HEADER_SIZE: usize = 12;
    const JUMBO_SIZE: usize = std::mem::size_of::<u16>();

    /// Creates a response to the provided packet header by
    /// changing the type of the header
    pub const fn response(&self) -> Self {
        self.with_type(PacketType::Response)
    }

    /// Copies the header contents changing its Packet Type
    ///
    /// `ty` The new packet type
    pub const fn with_type(&self, ty: PacketType) -> Self {
        Self {
            component: self.component,
            command: self.command,
            error: self.error,
            ty,
            options: PacketOptions::NONE,
            seq: self.seq,
        }
    }
    /// Creates a request header for the provided id, component
    /// and command
    ///
    /// `id`        The packet ID
    /// `component` The component to use
    /// `command`   The command to use
    pub const fn request(id: u16, component: u16, command: u16) -> Self {
        Self {
            component,
            command,
            error: 0,
            ty: PacketType::Request,
            options: PacketOptions::NONE,
            seq: id,
        }
    }

    pub fn path_matches(&self, other: &PacketHeader) -> bool {
        self.component.eq(&other.component) && self.command.eq(&other.command)
    }

    pub fn write(&self, dst: &mut BytesMut, length: usize) {
        let mut options = self.options;
        if length > 0xFFFF {
            options |= PacketOptions::JUMBO_FRAME;
        }

        dst.put_u8((length >> 8) as u8);
        dst.put_u8(length as u8);

        dst.put_u16(self.component);
        dst.put_u16(self.command);
        dst.put_u16(self.error);
        dst.put_u8((self.ty as u8) << 4);
        dst.put_u8(self.options.bits() << 4);
        dst.put_u16(self.seq);

        if self.options.contains(PacketOptions::JUMBO_FRAME) {
            dst.put_u8((length >> 24) as u8);
            dst.put_u8((length >> 16) as u8);
        }
    }

    pub fn read(src: &mut BytesMut) -> Option<(PacketHeader, usize)> {
        if src.len() < Self::MIN_HEADER_SIZE {
            return None;
        }

        let mut length = src.get_u16() as usize;
        let component = src.get_u16();
        let command = src.get_u16();
        let error = src.get_u16();
        let ty = src.get_u8() >> 4;
        let options = src.get_u8() >> 4;
        let options = PacketOptions::from_bits_retain(options);
        let seq = src.get_u16();

        if options.contains(PacketOptions::JUMBO_FRAME) {
            // We need another two bytes for the extended length
            if src.len() < Self::JUMBO_SIZE {
                return None;
            }
            let b1 = src.get_u8();
            let b2 = src.get_u8();
            length |= ((b1 as usize) << 24) | ((b2 as usize) << 16);
        }

        let ty = PacketType::from(ty);
        let header = PacketHeader {
            component,
            command,
            error,
            ty,
            options,
            seq,
        };
        Some((header, length))
    }
}

/// Structure for Blaze packets contains the contents of the packet
/// and the header for identification.
///
/// Packets can be cloned with little memory usage increase because
/// the content is stored as Bytes.
#[derive(Debug, Clone)]
pub struct Packet {
    /// The packet header
    pub header: PacketHeader,
    /// The packet encoded byte contents
    pub contents: Bytes,
}

fn serialize_bytes<V>(value: &V) -> Bytes
where
    V: TdfSerialize,
{
    Bytes::from(serialize_vec(value))
}

impl Packet {
    /// Creates a new packet from the provided header and contents
    pub const fn new(header: PacketHeader, contents: Bytes) -> Self {
        Self { header, contents }
    }

    /// Creates a new packet from the provided header with empty content
    #[inline]
    pub const fn new_empty(header: PacketHeader) -> Self {
        Self::new(header, Bytes::new())
    }

    #[inline]
    pub const fn new_request(id: u16, component: u16, command: u16, contents: Bytes) -> Packet {
        Self::new(PacketHeader::request(id, component, command), contents)
    }

    #[inline]
    pub const fn new_response(packet: &Packet, contents: Bytes) -> Self {
        Self::new(packet.header.response(), contents)
    }

    #[inline]
    pub const fn request_empty(id: u16, component: u16, command: u16) -> Packet {
        Self::new_empty(PacketHeader::request(id, component, command))
    }

    #[inline]
    pub const fn response_empty(packet: &Packet) -> Self {
        Self::new_empty(packet.header.response())
    }

    #[inline]
    pub fn response<V>(packet: &Packet, contents: V) -> Self
    where
        V: TdfSerialize,
    {
        Self::new_response(packet, serialize_bytes(&contents))
    }

    #[inline]
    pub fn request<V>(id: u16, component: u16, command: u16, contents: V) -> Packet
    where
        V: TdfSerialize,
    {
        Self::new_request(id, component, command, serialize_bytes(&contents))
    }

    /// Attempts to deserialize the packet contents as the provided type
    pub fn deserialize<'de, V>(&'de self) -> DecodeResult<V>
    where
        V: TdfDeserialize<'de>,
    {
        let mut r = TdfDeserializer::new(&self.contents);
        V::deserialize(&mut r)
    }

    pub fn read(src: &mut BytesMut) -> Option<Self> {
        let (header, length) = PacketHeader::read(src)?;

        if src.len() < length {
            return None;
        }

        let contents = src.split_to(length);
        Some(Self {
            header,
            contents: contents.freeze(),
        })
    }

    pub fn write(&self, dst: &mut BytesMut) {
        let contents = &self.contents;
        self.header.write(dst, contents.len());
        dst.extend_from_slice(contents);
    }
}

/// Tokio codec for encoding and decoding packets
pub struct PacketCodec;

impl Decoder for PacketCodec {
    type Error = io::Error;
    type Item = Packet;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut read_src = src.clone();
        let result = Packet::read(&mut read_src);

        if result.is_some() {
            *src = read_src;
        }

        Ok(result)
    }
}

impl Encoder<Packet> for PacketCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        item.write(dst);
        Ok(())
    }
}

/// Wrapper over a packet structure to provde debug logging
/// with names resolved for the component
pub struct PacketDebug<'a> {
    /// Reference to the packet itself
    pub packet: &'a Packet,
}

impl<'a> Debug for PacketDebug<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Append basic header information
        let header = &self.packet.header;

        let key = component_key(header.component, header.command);

        let is_notify = matches!(&header.ty, PacketType::Notify);
        let is_error = matches!(&header.ty, PacketType::Error);

        let component_name = get_component_name(header.component).unwrap_or("Unknown");
        let command_name = get_command_name(key, is_notify).unwrap_or("Unkown");

        write!(f, "{:?}", header.ty)?;

        if is_error {
            // Write sequence number and error for errors
            write!(f, " ({}, E?{:#06x})", header.seq, header.error)?;
        } else if !is_notify {
            // Write sequence number of sequenced types
            write!(f, " ({})", header.seq)?;
        }

        writeln!(
            f,
            ": {}->{} ({:#06x}->{:#06x})",
            component_name, command_name, header.component, header.command
        )?;

        writeln!(f, "Options: {:?}", header.options)?;
        write!(f, "Content: ")?;

        let r = TdfDeserializer::new(&self.packet.contents);
        let mut str = TdfStringifier::new(r, f);

        // Stringify the content or append error instead
        let _ = !str.stringify();
        // writeln!(
        //     &mut str.w,
        //     "\nRaw: {:?}",
        //     self.packet.contents.as_ref() as &[u8]
        // )
        Ok(())
    }
}
