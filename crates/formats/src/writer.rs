/// A buffer being built up, append only.
///
/// Nothing here is fallible. There's no seeking back, so a value that depends
/// on later data, like a total size, is computed before it's written.
#[derive(Debug, Default)]
pub struct Writer {
    data: Vec<u8>,
}

impl Writer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Also the position the next append lands at.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.data.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn u8(&mut self, value: u8) {
        self.data.push(value);
    }

    pub fn u16(&mut self, value: u16) {
        self.bytes(&value.to_be_bytes());
    }

    pub fn u32(&mut self, value: u32) {
        self.bytes(&value.to_be_bytes());
    }

    pub fn zeros(&mut self, len: usize) {
        self.data.resize(self.data.len().saturating_add(len), 0);
    }

    /// Pads with zeros to the next `to` boundary, if not already on one.
    pub fn align(&mut self, to: usize) {
        let len = self.data.len();
        self.zeros(len.next_multiple_of(to).saturating_sub(len));
    }

    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_go_out_big_endian() {
        let mut writer = Writer::new();
        writer.u8(0x0D);
        writer.u16(0xACED);
        writer.u32(0x0001_0203);
        assert_eq!(writer.finish(), [0x0D, 0xAC, 0xED, 0x00, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn padding_stops_on_the_next_boundary() {
        let mut writer = Writer::new();
        writer.bytes(b"abc");
        writer.align(4);
        assert_eq!(writer.len(), 4);
        writer.align(4);
        assert_eq!(writer.finish(), *b"abc\0");
    }
}
