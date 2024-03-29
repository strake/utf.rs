#![no_std]

#[cfg(test)]
extern crate std;

use core::{char::*, iter, num::NonZeroUsize, str};

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

pub fn decode_slice_u32(bs: &[u8]) -> Option<(u32, NonZeroUsize)> {
    let bs_l = bs.len();
    let (&b0, bs) = bs.split_first()?;
    let l = (!b0).leading_zeros() as usize;
    if l > bs_l { return None }
    if 0 == l { return Some((b0 as _, unsafe { NonZeroUsize::new_unchecked(1) })); }
    let l = NonZeroUsize::new(l)?;
    let mut x = b0 as u32 & (0x7F >> l.get());
    for b in bs.iter().cloned().take(l.get().wrapping_sub(1)) {
        x <<= 6;
        x |= b as u32 & 0x3F;
    }
    Some((x, l))
}

pub fn decode_slice(bs: &[u8]) -> Option<(char, NonZeroUsize)> {
    decode_slice_u32(bs).and_then(|(x, n)| from_u32(x).map(|x| (x, n)))
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
        static ls: [Fin7; 33] = [F0, F6, F6, F6, F6, F6, F5, F5,
                                 F5, F5, F5, F4, F4, F4, F4, F4,
                                 F3, F3, F3, F3, F3, F2, F2, F2,
                                 F2, F1, F1, F1, F1, F1, F1, F1, F1];
        let l = ls[self.leading_zeros() as usize] as usize;
        let first = !(!0u8 >> l);
        Some({
            let bs0 = bs.get_mut(0..l)?;
            let (b0, bs) = bs0.split_first_mut()?;
            for b in bs.iter_mut().rev() {
                *b = self as u8 & 0x3F | 0x80;
                self >>= 6;
            }
            *b0 = self as u8 | if l > 1 { first } else { 0 };
            bs0
        })
    }
}

/// Kludge until we have a stable version of `::core::intrinsics::assume`
#[derive(Clone, Copy)]
#[repr(u8)]
enum Fin7 { F0 = 0, F1 = 1, F2 = 2, F3 = 3, F4 = 4, F5 = 5, F6 = 6 }
use self::Fin7::*;

#[test]
fn test_decode() {
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
                       ("�A", &[0xFFu8, 0x41u8] as &[u8])].iter() {
        assert!(Iterator::eq(str.chars(),
                             decode_utf8(bs.into_iter().cloned())
                                 .map(|r_b| r_b.unwrap_or('\u{FFFD}'))),
                "chars = {}, bytes = {:?}, decoded = {:?}", str, bs,
                Vec::from_iter(decode_utf8(bs.into_iter().cloned())
                                   .map(|r_b| r_b.unwrap_or('\u{FFFD}'))));
    }
}

#[test]
fn test_encode() {
    for &(s, x) in
      [ ("A", 'A')
      , ("♥", '♥')
      ].iter() {
        let mut buf = [0u8; 6];
        let ts = x.try_encode_utf8(&mut buf[..]).map(|x| x as &str);
        assert_eq!(Some(s), ts, "{:02X?}", ts.map(str::as_bytes));
    }
}
