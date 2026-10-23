//! One byte arena for all node names; nodes keep only an offset and a length.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameRef {
    off: u64,
    len: u16,
}

#[derive(Debug, Default)]
pub struct NameArena {
    bytes: Vec<u8>,
}

impl NameArena {
    /// Names longer than `u16::MAX` bytes are truncated; `NAME_MAX` is 255 on Linux.
    pub fn push(&mut self, name: &[u8]) -> NameRef {
        let name = &name[..name.len().min(usize::from(u16::MAX))];
        let off = self.bytes.len() as u64;
        self.bytes.extend_from_slice(name);
        NameRef {
            off,
            len: name.len() as u16,
        }
    }

    pub fn get(&self, r: NameRef) -> &[u8] {
        let start = r.off as usize;
        &self.bytes[start..start + usize::from(r.len)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_returns_raw_bytes() {
        let mut a = NameArena::default();
        let x = a.push(b"abc");
        let y = a.push(b"\xff\n");
        let empty = a.push(b"");
        assert_eq!(a.get(x), b"abc");
        assert_eq!(a.get(y), b"\xff\n");
        assert_eq!(a.get(empty), b"");
        assert_eq!(a.get(NameRef::default()), b"");
    }

    #[test]
    fn overlong_names_are_truncated_not_panicking() {
        let mut a = NameArena::default();
        let r = a.push(&vec![b'x'; 70_000]);
        assert_eq!(a.get(r).len(), usize::from(u16::MAX));
    }
}
