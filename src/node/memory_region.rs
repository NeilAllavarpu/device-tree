//! Types to describe the physical memory present on a device, as specified under the root node of the Device Tree
use alloc::{boxed::Box, vec::Vec};

use super::{PropertyKeys, RawNode};
use crate::{map::Map, node_name::NameRef, parse::U32ByteSlice};
use core::{ffi::CStr, num::NonZeroU32};

/// An initially mapped area of memory provided by the bootloader.
/// Indicates a translation from effective addresses to physical address
#[derive(Debug, Clone)]
#[expect(
    clippy::exhaustive_structs,
    reason = "This is the format specified by the DTSpec"
)]
pub struct InitialMappedArea {
    /// The effective (virtual) address of this mapping
    pub effective_address: u64,
    /// The physical address corresponding to this mapping
    pub physical_address: u64,
    /// The size of the mapping
    pub size: NonZeroU32,
}

impl TryFrom<U32ByteSlice<'_>> for InitialMappedArea {
    type Error = ();

    #[inline]
    fn try_from(mut value: U32ByteSlice) -> Result<Self, Self::Error> {
        Ok(Self {
            effective_address: value.consume_u64().ok_or(())?,
            physical_address: value.consume_u64().ok_or(())?,
            size: value.try_into().ok().and_then(NonZeroU32::new).ok_or(())?,
        })
    }
}

/// A physical memory region
#[derive(Debug)]
pub struct MemoryRegion<'node> {
    /// The various regions of physical memory
    regions: Box<[(u64, u64)]>,
    /// Specifies an explicit hint to the operating system that this memory may potentially be removed later.
    hotpluggable: bool,
    /// Specifies the address and size of the Initial Mapped Area
    initial_mapped_area: Option<InitialMappedArea>,
    /// Miscellaneous other properties
    properties: Map<&'node CStr, U32ByteSlice<'node>>,
}

/// Errors from parsing a memory region
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// The type of the memory region was not "memory"
    Type,
    /// Error parsing the `Reg` field
    Reg,
    /// Unexpected children of this node
    Children,
}

impl<'node> MemoryRegion<'node> {
    /// Parses a memory node into a list of memory ranges with attributes
    pub(crate) fn new(
        mut node: RawNode<'node>,
        name: &NameRef<'node>,
        address_cells: u8,
        size_cells: u8,
    ) -> Result<Self, Error> {
        if !node.children.is_empty() {
            return Err(Error::Children);
        }

        if !node
            .properties
            .remove(PropertyKeys::DEVICE_TYPE)
            .is_some_and(|x| <&[u8]>::from(x) == b"memory\0")
        {
            return Err(Error::Type);
        }

        let hotpluggable = node.properties.remove(PropertyKeys::HOTPLUGGABLE).is_some();

        let mut bytes = node
            .properties
            .remove(PropertyKeys::REG)
            .ok_or(Error::Reg)?;

        let mut memory = Vec::new();

        while !bytes.is_empty() {
            let start = bytes.consume_cells(address_cells).ok_or(Error::Reg)?;
            let size = bytes.consume_cells(size_cells).ok_or(Error::Reg)?;

            if name.unit_address().is_some_and(|address| address != start) {
                return Err(Error::Reg);
            }
            memory.push((start, size));
        }
        Ok(MemoryRegion {
            regions: memory.into_boxed_slice(),
            hotpluggable,
            initial_mapped_area: None,
            properties: node.properties,
        })
    }

    #[inline]
    #[must_use]
    pub fn regions(&self) -> &[(u64, u64)] {
        self.regions.as_ref()
    }

    #[inline]
    #[must_use]
    pub const fn hotpluggable(&self) -> bool {
        self.hotpluggable
    }

    #[inline]
    #[must_use]
    pub const fn initial_mapped_area(&self) -> Option<&InitialMappedArea> {
        self.initial_mapped_area.as_ref()
    }

    #[inline]
    #[must_use]
    pub const fn properties(&self) -> &Map<&'node CStr, U32ByteSlice<'node>> {
        &self.properties
    }
}
