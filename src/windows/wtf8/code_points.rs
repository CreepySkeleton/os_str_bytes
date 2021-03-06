use std::iter::Fuse;
use std::mem;

use crate::error::EncodingError;

pub(super) const BYTE_SHIFT: u8 = 6;

pub(super) const CONT_MASK: u8 = (1 << BYTE_SHIFT) - 1;

pub(super) const CONT_TAG: u8 = 0b1000_0000;

#[derive(Debug)]
pub(in super::super) struct CodePoints<TIter> {
    iter: Fuse<TIter>,
    next: Option<u8>,
    surrogate: bool,
}

impl<TIter> CodePoints<TIter>
where
    TIter: Iterator<Item = u8>,
{
    pub(super) fn new<TString>(string: TString) -> Self
    where
        TString: IntoIterator<IntoIter = TIter, Item = TIter::Item>,
    {
        Self {
            iter: string.into_iter().fuse(),
            next: None,
            surrogate: false,
        }
    }

    pub(super) fn inner_iter(&self) -> &impl Iterator<Item = u8> {
        &self.iter
    }
}

impl<TIter> Iterator for CodePoints<TIter>
where
    TIter: Iterator<Item = u8>,
{
    type Item = Result<u32, EncodingError>;

    fn next(&mut self) -> Option<Self::Item> {
        let byte = self.next.or_else(|| self.iter.next())?;
        let mut code_point: u32 = byte.into();

        macro_rules! r#continue {
            () => {
                if let Some(byte) = self.iter.next() {
                    if byte & !CONT_MASK != CONT_TAG {
                        // Saving this byte will be useful if this crate ever
                        // offers a way to encode lossily.
                        self.next = Some(byte);
                        self.surrogate = false;
                        return Some(Err(EncodingError::Byte(byte)));
                    }
                    code_point = (code_point << BYTE_SHIFT)
                        | u32::from(byte & CONT_MASK);
                } else {
                    return Some(Err(EncodingError::End()));
                };
            };
        }

        let prev_surrogate = mem::replace(&mut self.surrogate, false);

        let mut invalid = false;
        if !byte.is_ascii() {
            if byte < 0xC2 {
                return Some(Err(EncodingError::Byte(byte)));
            }

            if byte < 0xE0 {
                code_point &= 0x1F;
            } else {
                code_point &= 0x0F;
                if byte >= 0xF0 {
                    r#continue!();
                    if code_point.wrapping_sub(0x10) > 0x100 {
                        invalid = true;
                    }
                }
                r#continue!();

                if code_point < 0x20 {
                    invalid = true;
                } else if code_point & 0xFE0 == 0x360 {
                    if code_point & 0x10 == 0 {
                        self.surrogate = true;
                    } else if prev_surrogate {
                        // This is a broken surrogate pair, so decoding it
                        // would be lossy.
                        invalid = true;
                    }
                }
            }
            r#continue!();
        }
        if invalid {
            return Some(Err(EncodingError::CodePoint(code_point)));
        }

        Some(Ok(code_point))
    }
}
