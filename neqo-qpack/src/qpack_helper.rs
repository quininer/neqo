use crate::{Error, Res};
use neqo_transport::Recvable;

pub trait ReadByte {
    fn read_byte(&mut self) -> Res<u8>;
}

struct ReceiverWrapper<'a> {
    receiver: &'a mut Recvable,
}

impl<'a> ReadByte for ReceiverWrapper<'a> {
    fn read_byte(&mut self) -> Res<u8> {
        let mut b = [0];
        let (amount, fin) = self.receiver.read(&mut b)?;
        if fin {
            return Err(Error::ClosedCriticalStream);
        }
        if amount != 1 {
            return Err(Error::NoMoreData);
        }
        Ok(b[0])
    }
}

pub struct BufWrapper<'a> {
    pub buf: &'a [u8],
    pub offset: usize,
}
impl<'a> BufWrapper<'a> {
    pub fn peek(&self) -> Res<u8> {
        if self.offset == self.buf.len() {
            Err(Error::DecompressionFailed)
        } else {
            Ok(self.buf[self.offset])
        }
    }

    pub fn slice(&mut self, len: usize) -> Res<&[u8]> {
        if self.offset + len > self.buf.len() {
            Err(Error::DecompressionFailed)
        } else {
            let start = self.offset;
            self.offset += len;
            Ok(&self.buf[start..self.offset])
        }
    }

    pub fn done(&self) -> bool {
        self.offset == self.buf.len()
    }
}

impl<'a> ReadByte for BufWrapper<'a> {
    fn read_byte(&mut self) -> Res<u8> {
        if self.offset == self.buf.len() {
            Err(Error::DecompressionFailed)
        } else {
            let b = self.buf[self.offset];
            self.offset += 1;
            Ok(b)
        }
    }
}

pub fn read_prefixed_encoded_int_with_recvable(
    s: &mut Recvable,
    val: &mut u64,
    cnt: &mut u8,
    prefix_len: u8,
    first_byte: u8,
    have_first_byte: bool,
) -> Res<bool> {
    let mut recv = ReceiverWrapper { receiver: s };
    match read_prefixed_encoded_int(&mut recv, val, cnt, prefix_len, first_byte, have_first_byte) {
        Ok(()) => Ok(true),
        Err(Error::NoMoreData) => Ok(false),
        Err(e) => Err(e),
    }
}

pub fn read_prefixed_encoded_int_slice(buf: &mut BufWrapper, prefix_len: u8) -> Res<u64> {
    assert!(prefix_len < 8);

    let first_byte = buf.read_byte()?;
    let mut val: u64 = 0;
    let mut cnt: u8 = 0;
    match read_prefixed_encoded_int(buf, &mut val, &mut cnt, prefix_len, first_byte, true) {
        Err(_) => Err(Error::DecompressionFailed),
        Ok(()) => Ok(val),
    }
}

pub fn read_prefixed_encoded_int(
    s: &mut ReadByte,
    val: &mut u64,
    cnt: &mut u8,
    prefix_len: u8,
    first_byte: u8,
    have_first_byte: bool,
) -> Res<()> {
    if have_first_byte {
        let mask = if prefix_len == 0 {
            0xff
        } else {
            (1 << (8 - prefix_len)) - 1
        };
        *val = (first_byte & mask) as u64;

        if *val < mask as u64 {
            return Ok(());
        }
    }
    let mut b: u8;
    loop {
        b = s.read_byte()?;

        if (*cnt == 63) && (b > 1 || (b == 1 && ((*val >> 63) == 1))) {
            break Err(Error::IntegerOverflow);
        }
        *val += ((b & 0x7f) as u64) << *cnt;
        if (b & 0x80) == 0 {
            break Ok(());
        }
        *cnt += 7;
        if *cnt >= 64 {
            break Ok(());
        }
    }
}
