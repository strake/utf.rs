#![no_std]

#[cfg(test)]
extern crate std;

use core::{char::*, iter, str};

/// An iterator over an iterator of bytes of the characters the bytes represent
/// as UTF-8
#[derive(Clone, Debug)]
pub struct DecodeUtf8<I: Iterator<Item = u8>>(iter::Peekable<I>);

/// Decodes an `Iterator` of bytes as UTF-8.
#[inline]
pub fn decode_utf8<I: IntoIterator<Item = u8>>(i: I) -> DecodeUtf8<I::IntoIter> {
    DecodeUtf8(i.into_iter().peekable())
}

/// `<DecodeUtf8 as Iterator>::next` returns this for an invalid input sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InvalidSequence(());

impl<I: Iterator<Item = u8>> Iterator for DecodeUtf8<I> {
    type Item = Result<char, InvalidSequence>;
    #[inline]
    fn next(&mut self) -> Option<Result<char, InvalidSequence>> {
        self.0.next().map(|b| {
            if b & 0x80 == 0 { Ok(b as char) } else {
                let l = (!b).leading_zeros() as usize; // number of bytes in UTF-8 representation
                if l < 2 || l > 6 { return Err(InvalidSequence(())) };
                let mut x = (b as u32) & (0x7F >> l);
                for _ in 0..l-1 {
                    match self.0.peek() {
                        Some(&b) if b & 0xC0 == 0x80 => {
                            self.0.next();
                            x = (x << 6) | (b as u32) & 0x3F;
                        },
                        _ => return Err(InvalidSequence(())),
                    }
                }
                match from_u32(x) {
                    Some(x) if l == x.len_utf8() => Ok(x),
                    _ => Err(InvalidSequence(())),
                }
            }
        })
    }
}

mod private {
    pub trait UtfExtSealed {}
}
use private::*;

pub trait UtfExt: UtfExtSealed {
    type UtfSlice: ?Sized;
    /// Encode the character into the given buffer; return `None` if the buffer is too short.
    fn try_encode_utf8(self, bs: &mut [u8]) -> Option<&mut Self::UtfSlice>;
}

impl UtfExtSealed for char {}
impl UtfExtSealed for u32 {}

impl UtfExt for char {
    type UtfSlice = str;
    #[inline]
    fn try_encode_utf8(self, bs: &mut [u8]) -> Option<&mut str> {
        (self as u32).try_encode_utf8(bs).map(|bs| unsafe { str::from_utf8_unchecked_mut(bs) })
    }
}

impl UtfExt for u32 {
    type UtfSlice = [u8];
    fn try_encode_utf8(mut self, bs: &mut [u8]) -> Option<&mut [u8]> {
        let (first, l) =
        if self >= 1 << 26 { (0xFC, 6) } else
        if self >= 1 << 21 { (0xF8, 5) } else
        if self >= 1 << 16 { (0xF0, 4) } else
        if self >= 1 << 11 { (0xE0, 3) } else
        if self >= 1 <<  7 { (0xC0, 2) } else
                           { (0x00, 1) };
        let bs = bs.get_mut(0..l)?;
        for bp in bs.iter_mut().skip(1).rev() {
            *bp = self as u8 & 0x3F | 0x80;
            self >>= 6;
        }
        bs[0] = self as u8 | first;
        Some(bs)
    }
}

#[test]
fn test() {
    use std::vec::Vec;
    use std::iter::FromIterator;

    for &(str, bs) in [("", &[] as &[u8]),
                       ("A", &[0x41u8] as &[u8]),
                       ("�", &[0xC1u8, 0x81u8] as &[u8]),
                       ("♥", &[0xE2u8, 0x99u8, 0xA5u8]),
                       ("♥A", &[0xE2u8, 0x99u8, 0xA5u8, 0x41u8] as &[u8]),
                       ("�", &[0xE2u8, 0x99u8] as &[u8]),
                       ("�A", &[0xE2u8, 0x99u8, 0x41u8] as &[u8]),
                       ("�", &[0xC0u8] as &[u8]),
                       ("�A", &[0xC0u8, 0x41u8] as &[u8]),
                       ("�", &[0x80u8] as &[u8]),
                       ("�A", &[0x80u8, 0x41u8] as &[u8]),
                       ("�", &[0xFEu8] as &[u8]),
                       ("�A", &[0xFEu8, 0x41u8] as &[u8]),
                       ("�", &[0xFFu8] as &[u8]),
                       ("�A", &[0xFFu8, 0x41u8] as &[u8])].into_iter() {
        assert!(Iterator::eq(str.chars(),
                             decode_utf8(bs.into_iter().cloned())
                                 .map(|r_b| r_b.unwrap_or('\u{FFFD}'))),
                "chars = {}, bytes = {:?}, decoded = {:?}", str, bs,
                Vec::from_iter(decode_utf8(bs.into_iter().cloned())
                                   .map(|r_b| r_b.unwrap_or('\u{FFFD}'))));
    }
}
