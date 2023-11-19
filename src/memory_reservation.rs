#[derive(Debug)]
pub struct MemoryReservations(pub Box<[(u64, u64)]>);

impl TryFrom<&[u64]> for MemoryReservations {
    type Error = ();

    fn try_from(value: &[u64]) -> Result<Self, Self::Error> {
        if value.len() % 2 != 0 {
            return Err(());
        }

        let mut entries: Box<_> = value
            .array_chunks::<2>()
            .map(|&[address, size]| (u64::from_be(address), u64::from_be(size)))
            .collect();
        entries.sort_unstable();
        Ok(Self(entries))
    }
}
