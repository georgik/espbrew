//! Image format handling structures

use std::borrow::Cow;

/// A segment of code from the source ELF
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Segment<'a> {
    /// Base address of the code segment
    pub addr: u32,
    /// Segment data
    pub data: Cow<'a, [u8]>,
}

impl<'a> Segment<'a> {
    /// Creates a new [`Segment`].
    pub fn new(addr: u32, data: &'a [u8]) -> Self {
        Segment {
            addr,
            data: Cow::Borrowed(data),
        }
    }

    /// Return the size of the segment
    pub fn size(&self) -> u32 {
        self.data.len() as u32
    }

    /// Return the data of the segment
    pub fn data(&self) -> &[u8] {
        self.data.as_ref()
    }
}

impl<'a> std::fmt::Debug for Segment<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Segment")
            .field("addr", &format!("0x{:x}", self.addr))
            .field("size", &self.size())
            .finish()
    }
}

/// Image format enum for different ESP formats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageFormat<'a> {
    /// ESP-IDF application image format
    EspIdf(crate::espflash_local::IdfBootloaderFormat<'a>),
}

impl<'a> ImageFormat<'a> {
    /// Returns all flashable data segments
    pub fn flash_segments(self) -> Vec<Segment<'a>> {
        match self {
            ImageFormat::EspIdf(idf) => idf.flash_segments(),
        }
    }
}
