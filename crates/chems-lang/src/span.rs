use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl ByteSpan {
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    #[must_use]
    pub const fn empty(at: usize) -> Self {
        Self::new(at, at)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePosition {
    pub line: usize,
    pub scalar_column: usize,
}

impl SourcePosition {
    #[must_use]
    pub fn at(source: &str, byte_offset: usize) -> Self {
        let mut offset = byte_offset.min(source.len());
        while !source.is_char_boundary(offset) {
            offset -= 1;
        }
        let bytes = source.as_bytes();
        let mut line = 0;
        let mut line_start = 0;
        let mut index = 0;
        while index < offset {
            if bytes[index] == b'\r' {
                line += 1;
                index += usize::from(index + 1 < offset && bytes[index + 1] == b'\n') + 1;
                line_start = index;
            } else if bytes[index] == b'\n' {
                line += 1;
                index += 1;
                line_start = index;
            } else {
                index += 1;
            }
        }
        let scalar_column = source[line_start..offset].chars().count();
        Self {
            line,
            scalar_column,
        }
    }
}
