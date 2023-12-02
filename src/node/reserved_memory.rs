//! Types to describe regions of memory that are reserved and must be handled specially by the OS or other programs
//!
//! This is different from the memory reservations described in the DTB that are not part of the device tree directly

use alloc::rc::Rc;

use super::{device, ChildMap, PropertyMap, RawNode, RawNodeError};
use crate::map::Map;
use crate::node_name::NameRef;
use crate::{node::PropertyKeys, split_at_first};
use core::{ffi::CStr, num::NonZeroU8};
use core::{fmt, str};

/// Additional information regarding the usage intent of a given reserved region of memory
#[expect(
    clippy::exhaustive_enums,
    reason = "These are the only possible variants as specified by the Device Tree spec"
)]
pub enum Compatible<'bytes> {
    /// This indicates a region of memory meant to be used as a shared pool of DMA buffers for a set of devices.
    /// It can be used by an operating system to instantiate the necessary pool management subsystem if necessary.
    SharedDmaPool,
    /// A vendor specific string of vendor, optional device, and usage
    VendorSpecific(&'bytes [u8], Option<&'bytes [u8]>, &'bytes [u8]),
}

impl fmt::Debug for Compatible<'_> {
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::SharedDmaPool => write!(formatter, "SharedDmaPool"),
            Self::VendorSpecific(vendor, device, usage) => {
                let mut tuple = formatter.debug_tuple("VendorSpecific");
                if let Ok(vendor_str) = str::from_utf8(vendor) {
                    tuple.field(&vendor_str);
                } else {
                    tuple.field(&vendor);
                }

                if let Ok(device_str) = device.map(str::from_utf8).transpose() {
                    tuple.field(&device_str);
                } else {
                    tuple.field(&device);
                }

                if let Ok(usage_str) = str::from_utf8(usage) {
                    tuple.field(&usage_str);
                } else {
                    tuple.field(&usage);
                }
                tuple.finish()
            }
        }
    }
}

impl<'bytes> TryFrom<&'bytes CStr> for Compatible<'bytes> {
    type Error = ();

    #[inline]
    fn try_from(value: &'bytes CStr) -> Result<Self, Self::Error> {
        let bytes = value.to_bytes();
        if bytes == b"shared-dma-pool" {
            Ok(Self::SharedDmaPool)
        } else {
            let (vendor, remainder) = split_at_first(bytes, &b',').ok_or(())?;
            split_at_first(remainder, &b'-').map_or(
                Ok(Self::VendorSpecific(vendor, None, remainder)),
                |(device, usage)| Ok(Self::VendorSpecific(vendor, Some(device), usage)),
            )
        }
    }
}

/// Describes the limitations for a region of reserved memory
#[derive(Debug)]
#[expect(
    clippy::exhaustive_enums,
    reason = "These are the only possible variants as specified by the Device Tree spec"
)]
pub enum Usage {
    /// Indicates the operating system must not create a virtual mapping of the region as part of its standard mapping of system memory,
    /// nor permit speculative access to it under any circumstances other than under the control of the device driver using the region.
    NoMap,
    /// The operating system can use the memory in this region with the limitation that the device driver(s) owning the region need to be able to reclaim it back.
    /// Typically that means that the operating system can use that region to store volatile or cached data that can be otherwise regenerated or migrated elsewhere.
    Reusable,
    /// No special mark for the usage of this memory
    Other,
}

#[derive(Debug)]
/// A reserved memory node requires either a `reg` property for static allocations, or a `size` property for dynamics allocations. If both reg and size are present, then the region is treated as a static allocation with the `reg` property taking precedence and `size` is ignored.
#[expect(
    clippy::exhaustive_enums,
    reason = "These are the only possible variants as specified by the Device Tree spec"
)]
pub enum Range {
    /// Consists of an arbitrary number of address and size pairs that specify the physical address and size of the memory ranges.
    Static(Box<[(u64, u64)]>),
    /// Dynamic allocations may use `alignment` and `alloc-ranges` properties to constrain where the memory is allocated from.
    Dynamic(u64, Option<u64>, Option<Box<[(u64, u64)]>>),
}

/// Each child of the reserved-memory node specifies one or more regions of reserved memory.
/// Each child node may either use a `reg` property to specify a specific range of reserved memory, or a `size` property with optional constraints to request a dynamically allocated block of memory.
///
/// Following the generic-names recommended practice, node names should reflect the purpose of the node (ie. “framebuffer” or “dma-pool”). Unit address (`@<address>`) should be appended to the name if the node is a static allocation.
#[derive(Debug)]
pub struct Node<'node> {
    /// The range of memory that this reservation describes
    memory: Range,
    /// The usage permitted for this range of memory
    usage: Usage,
    /// Additional information about the usage of this memory
    compatible: Option<Compatible<'node>>,
    /// Other miscellaneous properties
    properties: PropertyMap<'node>,
    /// Any children, if present
    children: ChildMap<'node>,
}

/// Errors that can occur when parsing a reserved memory node
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Error parsing either the static or dynamic allocation
    InvalidMemory,
    /// Error parsing the usage: both `no_map` and `reusable` were specified
    Usage,
    /// Invalid cells provided while attempting to parse
    Cells,
    /// Error parsing the compatibility field
    Compatible,
    /// Error parsing a child
    Child(device::Error),
}

#[derive(Debug)]
#[non_exhaustive]
pub enum RootError {
    Child(Error),
    CellsMismatch,
}

impl<'node> Node<'node> {
    /// Parses the given raw node into a reserved memory node
    pub(crate) fn new(
        mut value: RawNode<'node>,
        address_cells: u8,
        size_cells: NonZeroU8,
        phandles: &mut Map<u32, Rc<device::Node<'node>>>,
    ) -> Result<Self, Error> {
        let size = value
            .properties
            .remove(PropertyKeys::SIZE)
            .map(|bytes| bytes.into_cells(size_cells.get()).ok_or(Error::Cells))
            .transpose()?;
        let alignment = value
            .properties
            .remove(PropertyKeys::ALIGNMENT)
            .map(|bytes| bytes.into_cells(size_cells.get()).ok_or(Error::Cells))
            .transpose()?;

        let regs = value
            .properties
            .remove(PropertyKeys::REG)
            .map(|mut reg| {
                let mut regs = Vec::new();
                while !reg.is_empty() {
                    regs.push((
                        reg.consume_cells(address_cells).ok_or(Error::Cells)?,
                        reg.consume_cells(size_cells.get()).ok_or(Error::Cells)?,
                    ));
                }
                Ok(regs.into_boxed_slice())
            })
            .transpose()?;

        let no_map = value.properties.remove(PropertyKeys::NO_MAP).is_some();
        let reusable = value.properties.remove(PropertyKeys::REUSABLE).is_some();

        if no_map && reusable {
            return Err(Error::Usage);
        }

        let alloc_ranges = value
            .properties
            .remove(PropertyKeys::ALLOC_RANGES)
            .map(|mut ranges| {
                let mut reg = Vec::new();
                while !ranges.is_empty() {
                    reg.push((
                        ranges.consume_cells(address_cells).ok_or(Error::Cells)?,
                        ranges.consume_cells(size_cells.get()).ok_or(Error::Cells)?,
                    ));
                }
                reg.sort_unstable_by_key(|&(start, _)| start);
                Ok(reg.into_boxed_slice())
            })
            .transpose()?;

        let compatible = value
            .properties
            .remove(PropertyKeys::COMPATIBLE)
            .map(|bytes| {
                <&CStr>::try_from(bytes)
                    .ok()
                    .and_then(|x| Compatible::try_from(x).ok())
                    .ok_or(Error::Compatible)
            })
            .transpose()?;

        let (properties, children) = value.into_components(phandles, None);
        let children = match children {
            Ok(children) => children,
            Err(RawNodeError::Cells) => return Err(Error::Cells),
            Err(RawNodeError::Child(child)) => return Err(Error::Child(child)),
        };

        Ok(Self {
            memory: regs.map_or_else(
                || {
                    size.map(|x| Range::Dynamic(x, alignment, alloc_ranges))
                        .ok_or(Error::InvalidMemory)
                },
                |static_regs| Ok(Range::Static(static_regs)),
            )?,
            compatible,
            usage: if no_map {
                Usage::NoMap
            } else if reusable {
                Usage::Reusable
            } else {
                Usage::Other
            },
            properties,
            children,
        })
    }

    /// Parses the parent `/reserved-memory` node and returns all the associated reserved memory
    pub(super) fn parse_parent(
        mut parent: RawNode<'node>,
        address_cells: u8,
        size_cells: NonZeroU8,
        phandles: &mut Map<u32, Rc<device::Node<'node>>>,
    ) -> Result<Map<NameRef<'node>, Self>, RootError> {
        // #address-cells and #size-cells should use the same values as for the root node, and ranges should be empty so that address translation logic works correctly.
        let (reserved_memory_addr_cells, reserved_memory_size_cells) = parent.extract_cell_counts();

        if !(reserved_memory_addr_cells.is_ok_and(|cells| cells == address_cells)
            && reserved_memory_size_cells.is_ok_and(|cells| cells == size_cells.get())
            && parent
                .properties
                .remove(PropertyKeys::RANGES)
                .is_some_and(|ranges| ranges.is_empty()))
        {
            return Err(RootError::CellsMismatch);
        }

        parent
            .children
            .into_iter()
            .map(|(name, node)| {
                Node::new(node, address_cells, size_cells, phandles)
                    .map(|reserved_node| (name, reserved_node))
            })
            .try_collect()
            .map_err(RootError::Child)
    }

    #[inline]
    #[must_use]
    pub const fn memory(&self) -> &Range {
        &self.memory
    }

    #[inline]
    #[must_use]
    pub const fn usage(&self) -> &Usage {
        &self.usage
    }

    #[inline]
    #[must_use]
    pub const fn compatible(&self) -> Option<&Compatible<'_>> {
        self.compatible.as_ref()
    }
}

impl<'node> super::Node<'node> for Node<'node> {
    #[inline]
    fn properties(&self) -> &PropertyMap {
        &self.properties
    }

    #[inline]
    fn children(&self) -> &ChildMap<'node> {
        &self.children
    }
}
