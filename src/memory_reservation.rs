//! The memory reservation block provides the client program with a list of areas in physical memory which are reserved; that is, which shall not be used for general memory allocations. It is used to protect vital data structures from being overwritten by the client program. For example, on some systems with an IOMMU, the TCE (translation control entry) tables initialized by a `DTSpec` boot program would need to be protected in this manner. Likewise, any boot program code or data used during the client program’s runtime would need to be reserved (e.g., RTAS on Open Firmware platforms). `DTSpec` does not require the boot program to provide any such runtime components, but it does not prohibit implementations from doing so as an extension.
//!
//! More specifically, a client program shall not access memory in a reserved region unless other information provided by the boot program explicitly indicates that it shall do so. The client program may then access the indicated section of the reserved memory in the indicated manner. Methods by which the boot program can indicate to the client program specific uses for reserved memory may appear in the device tree specification, in optional extensions to it, or in platform-specific documentation.
//!
//! The reserved regions supplied by a boot program may, but are not required to, encompass the devicetree blob itself. The client program shall ensure that it does not overwrite this data structure before it is used, whether or not it is in the reserved areas.
//!
//! Any memory that is declared in a memory node and is accessed by the boot program or caused to be accessed by the boot program after client entry must be reserved. Examples of this type of access include (e.g., speculative memory reads through a non-guarded virtual page).
//!
//! This requirement is necessary because any memory that is not reserved may be accessed by the client program with arbitrary storage attributes.
//!
//! Any accesses to reserved memory by or caused by the boot program must be done as not Caching Inhibited and Memory Coherence Required (i.e., WIMG = 0bx01x), and additionally for Book III-S implementations as not Write Through Required (i.e., WIMG = 0b001x). Further, if the VLE storage attribute is supported, all accesses to reserved memory must be done as VLE=0.
//!
//! This requirement is necessary because the client program is permitted to map memory with storage attributes specified as not Write Through Required, not Caching Inhibited, and Memory Coherence Required (i.e., WIMG = 0b001x), and VLE=0 where supported. The client program may use large virtual pages that contain reserved memory. However, the client program may not modify reserved memory, so the boot program may perform accesses to reserved memory as Write Through Required where conflicting values for this storage attribute are architecturally permissible.

use alloc::{boxed::Box, vec::Vec};

/// Each pair gives the physical address and size in bytes of a reserved memory region. These given regions shall not overlap each other. The list of reserved blocks shall be terminated with an entry where both address and size are equal to 0.

#[derive(Debug)]
pub struct MemoryReservations(pub Box<[(u64, u64)]>);

impl TryFrom<&[u64]> for MemoryReservations {
    type Error = ();

    fn try_from(value: &[u64]) -> Result<Self, Self::Error> {
        if value.len() % 2 != 0 {
            return Err(());
        }

        let mut entries: Vec<_> = value
            .array_chunks::<2>()
            .map(|&[address, size]| (u64::from_be(address), u64::from_be(size)))
            .collect();

        if entries.pop() != Some((0, 0)) {
            return Err(());
        }
        entries.sort_unstable();

        Ok(Self(entries.into_boxed_slice()))
    }
}
