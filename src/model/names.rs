//! One byte arena for all node names; nodes keep only an offset and a length.

/// Bytes per block of the arena.
pub const NAME_BLOCK: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameRef {
    off: u64,
    len: u16,
}

/// Names live in fixed blocks of `NAME_BLOCK` bytes that never move once allocated; a
/// name never straddles two blocks. `NameRef::off` is `block * NAME_BLOCK + position`.
#[derive(Debug, Default)]
pub struct NameArena {
    blocks: Vec<Vec<u8>>,
}

impl NameArena {
    /// Names longer than `u16::MAX` bytes are truncated; `NAME_MAX` is 255 on Linux.
    pub fn push(&mut self, name: &[u8]) -> NameRef {
        let name = &name[..name.len().min(usize::from(u16::MAX))];
        let fits = self
            .blocks
            .last()
            .is_some_and(|b| b.capacity() - b.len() >= name.len());
        if !fits {
            self.blocks.push(Vec::with_capacity(NAME_BLOCK));
        }
        let block = self.blocks.len() - 1;
        let bytes = &mut self.blocks[block];
        let off = (block * NAME_BLOCK + bytes.len()) as u64;
        bytes.extend_from_slice(name);
        NameRef {
            off,
            len: name.len() as u16,
        }
    }

    pub fn get(&self, r: NameRef) -> &[u8] {
        let (block, start) = (r.off as usize / NAME_BLOCK, r.off as usize % NAME_BLOCK);
        self.blocks
            .get(block)
            .map_or(&[][..], |b| &b[start..start + usize::from(r.len)])
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
    fn growing_never_moves_names_already_stored() {
        let mut a = NameArena::default();
        let first = a.push(b"first-name");
        let at = a.get(first).as_ptr();
        // Several blocks' worth of names.
        let mut refs = Vec::new();
        for i in 0..40_000u32 {
            refs.push((
                i,
                a.push(format!("name-number-{i:08}-{}", "x".repeat(60)).as_bytes()),
            ));
        }
        assert_eq!(a.get(first).as_ptr(), at, "the first name did not move");
        assert_eq!(a.get(first), b"first-name");
        for (i, r) in refs.iter().step_by(997) {
            assert_eq!(
                a.get(*r),
                format!("name-number-{i:08}-{}", "x".repeat(60)).as_bytes()
            );
        }
    }

    #[test]
    fn a_name_never_straddles_two_blocks() {
        let mut a = NameArena::default();
        // Fill the first block almost completely, then push a name that does not fit.
        let filler = vec![b'f'; NAME_BLOCK - 10];
        let r1 = a.push(&filler[..60_000]);
        let r2 = a.push(&filler[..60_000]);
        let r3 = a.push(b"0123456789abcdef");
        assert_eq!(a.get(r1).len() + a.get(r2).len(), 120_000);
        assert_eq!(a.get(r3), b"0123456789abcdef");
    }

    #[test]
    fn overlong_names_are_truncated_not_panicking() {
        let mut a = NameArena::default();
        let r = a.push(&vec![b'x'; 70_000]);
        assert_eq!(a.get(r).len(), usize::from(u16::MAX));
    }
}
