// These iterators convert between wide characters and potentially invalid
// WTF-8:
// https://simonsapin.github.io/wtf-8/

use std::char;
use std::char::DecodeUtf16;
use std::char::DecodeUtf16Error;

use crate::error::EncodingError;

mod code_points;
use code_points::CodePoints;
use code_points::BYTE_SHIFT;
use code_points::CONT_MASK;
use code_points::CONT_TAG;

const MIN_HIGH_SURROGATE: u16 = 0xD800;

const MIN_LOW_SURROGATE: u16 = 0xDC00;

const MIN_SURROGATE_CODE: u32 = (u16::max_value() as u32) + 1;

#[derive(Debug)]
pub(super) struct DecodeWide<TIter> {
    iter: TIter,
    code_point: Option<u32>,
    shift: u8,
}

impl<TIter> DecodeWide<DecodeUtf16<TIter>>
where
    TIter: Iterator<Item = u16>,
{
    pub(super) fn new<TString>(string: TString) -> Self
    where
        TString: IntoIterator<IntoIter = TIter, Item = TIter::Item>,
    {
        Self {
            iter: char::decode_utf16(string),
            code_point: None,
            shift: 0,
        }
    }
}

impl<TIter> Iterator for DecodeWide<TIter>
where
    TIter: Iterator<Item = Result<char, DecodeUtf16Error>>,
{
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(code_point) = self.code_point {
            if let Some(shift) = self.shift.checked_sub(BYTE_SHIFT) {
                self.shift = shift;
                return Some(
                    ((code_point >> self.shift) as u8 & CONT_MASK) | CONT_TAG,
                );
            }
        }
        debug_assert_eq!(0, self.shift);

        let code_point = self
            .iter
            .next()?
            .map(Into::into)
            .unwrap_or_else(|x| x.unpaired_surrogate().into());
        self.code_point = Some(code_point);

        macro_rules! try_decode {
            ( $tag:expr ) => {
                Some((code_point >> self.shift) as u8 | $tag)
            };
            ( $tag:expr , $upper_bound:expr ) => {
                if code_point < $upper_bound {
                    return try_decode!($tag);
                }
                self.shift += BYTE_SHIFT;
            };
        }
        try_decode!(0, 0x80);
        try_decode!(0xC0, 0x800);
        try_decode!(0xE0, MIN_SURROGATE_CODE);
        try_decode!(0xF0)
    }
}

#[derive(Debug)]
pub(super) struct EncodeWide<TIter> {
    iter: TIter,
    surrogate: Option<u16>,
}

impl<TIter> EncodeWide<CodePoints<TIter>>
where
    TIter: Iterator<Item = u8>,
{
    pub(super) fn new<TString>(string: TString) -> Self
    where
        TString: IntoIterator<IntoIter = TIter, Item = TIter::Item>,
    {
        Self {
            iter: CodePoints::new(string),
            surrogate: None,
        }
    }
}

impl<TIter> Iterator for EncodeWide<CodePoints<TIter>>
where
    TIter: Iterator<Item = u8>,
{
    type Item = Result<u16, EncodingError>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(surrogate) = self.surrogate.take() {
            return Some(Ok(surrogate));
        }

        self.iter.next().map(|code_point| {
            code_point.map(|code_point| {
                if let Some(offset) =
                    code_point.checked_sub(MIN_SURROGATE_CODE)
                {
                    self.surrogate =
                        Some((offset & 0x3FF) as u16 | MIN_LOW_SURROGATE);
                    (offset >> 10) as u16 | MIN_HIGH_SURROGATE
                } else {
                    code_point as u16
                }
            })
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size_hint = self.iter.inner_iter().size_hint();
        (size_hint.0.saturating_add(2) / 3, size_hint.1)
    }
}
