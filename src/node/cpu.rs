//! A CPU node. CPU nodes describe the physical cores that are present.

use crate::{
    map::Map,
    parse::U32ByteSlice,
    property::{EnableMethod, EnableMethodError},
};
use alloc::rc::Rc;
use core::{ffi::CStr, num::NonZeroU8};

use super::{
    cache::{HigherLevel, HigherLevelError, L1},
    device,
    root::NodeNames,
    PropertyKeys, RawNode,
};

/// Status of a CPU as indicated by the node
#[derive(Debug)]
#[non_exhaustive]
pub enum Status {
    /// The CPU is running.
    Okay,
    /// The CPU is in a quiescent state.
    ///
    /// A quiescent CPU is in a state where it cannot interfere with the normal operation of other CPUs,
    /// nor can its state be affected by the normal operation of other running CPUs,
    /// except by an explicit method for enabling or reenabling the quiescent CPU.
    ///
    /// In particular, a running CPU shall be able to issue broadcast TLB invalidates without affecting a quiescent CPU.
    ///
    /// Examples: A quiescent CPU could be in a spin loop, held in reset,
    /// and electrically isolated from the system bus or in another implementation dependent state.
    Disabled,
    /// The CPU is not operational or does not exist.
    ///
    /// A CPU with `Fail` status does not affect the system in any way.
    /// The status is assigned to nodes for which no corresponding CPU exists.
    Fail,
}

/// A node representing a physical CPU
#[derive(Debug)]
pub struct Node<'node> {
    /// The mechanism for enabling a CPU. Required if `status` is `Fail`
    enable_method: Option<EnableMethod<'node>>,
    /// A unique identifier for this CPU
    pub(crate) reg: u32,
    /// The L1 cache for this CPU
    l1_cache: L1,
    /// The status of this CPU. If `Disabled`, can be enabled via the mechanism described by `enable_method`
    status: Status,
    /// The next level cache after L1 for this CPU, if present
    next_cache: Option<Rc<HigherLevel<'node>>>,
    /// Miscellaneous other properties for this CPU
    properties: Map<&'node CStr, U32ByteSlice<'node>>,
}

/// Errors from attempting to parse a `CpuNode`
#[derive(Debug)]
#[non_exhaustive]
pub enum NodeError {
    /// Error parsing the device type, or the device type was not 0
    DeviceType,
    /// Error parsing the enable method (but not the release address of a spin table)
    EnableMethod,
    /// Error parsing the release address of a spin table
    ReleaseAddr,
    /// Error parsing the status of the CPU
    Status,
    /// Error parsing the `reg` field of the node
    Reg,
    /// Next-level cache is a dangling phandle
    NextLevelCache,
}

/// Errors from attempting to parse the parent `/cpus` node
#[non_exhaustive]
#[derive(Debug)]
pub enum RootError {
    /// Error parsing a child CPU
    Cpu(NodeError),
    /// Error parsing a child cache
    Cache(HigherLevelError),
    /// Missing a field for address/size cells
    Reg,
    /// Mismatch between a child CPU's specified reg and its unit-address
    RegMismatch(Option<u64>, u32),
}

/// A map of CPU IDs to CPU nodes
type CpuMap<'node> = Map<u32, Rc<Node<'node>>>;
/// A map of cache IDs to cache Nodes
type CacheMap<'node> = Map<u32, Rc<HigherLevel<'node>>>;

impl<'node> Node<'node> {
    /// Parses and creates a CPU node from the provided informaiton
    fn new<'parsing>(
        mut value: RawNode<'node>,
        base: &'parsing Map<&'node CStr, U32ByteSlice<'node>>,
        cache_handles: &'parsing Map<u32, Rc<HigherLevel<'node>>>,
        address_cells: NonZeroU8,
    ) -> Result<Self, NodeError> {
        value.properties.extend_preserve(base);

        if !value
            .properties
            .remove(PropertyKeys::DEVICE_TYPE)
            .is_some_and(|device_type| {
                <&CStr>::try_from(device_type).is_ok_and(|x| x.to_bytes() == b"cpu")
            })
        {
            return Err(NodeError::DeviceType);
        }

        let enable_method = match EnableMethod::extract_from_properties(&mut value.properties) {
            Ok(method) => Some(method),
            Err(EnableMethodError::NotPresent) => None,
            Err(EnableMethodError::NoReleaseAddr) => return Err(NodeError::ReleaseAddr),
            Err(EnableMethodError::Invalid) => return Err(NodeError::EnableMethod),
        };

        let status = {
            let property = value.properties.remove(PropertyKeys::STATUS);
            if let Some(property) = property {
                match <&CStr>::try_from(property).map(CStr::to_bytes) {
                    Ok(b"okay") => Status::Okay,
                    Ok(b"disabled") => {
                        if enable_method.is_none() {
                            return Err(NodeError::EnableMethod);
                        }
                        Status::Disabled
                    }
                    Ok(b"fail") => Status::Fail,
                    _ => return Err(NodeError::Status),
                }
            } else {
                Status::Okay
            }
        };

        let cache = L1::extract_from(&mut value.properties);

        let next_cache = value
            .properties
            .remove(PropertyKeys::NEXT_LEVEL_CACHE)
            .map(|phandle| {
                u32::try_from(phandle)
                    .ok()
                    .and_then(|x| cache_handles.get(&x).cloned())
                    .ok_or(NodeError::NextLevelCache)
            })
            .transpose()?;

        let reg = value
            .properties
            .remove(PropertyKeys::REG)
            .and_then(|bytes| {
                if address_cells.get() != 1 {
                    unimplemented!("Only 32-bit CPU IDs are currently supported");
                }
                u32::try_from(bytes).ok()
            })
            .ok_or(NodeError::Reg)?;

        Ok(Self {
            reg,
            enable_method,
            l1_cache: cache,
            next_cache,
            status,
            properties: value.properties,
        })
    }

    /// Parses the parent CPU node and returns a map describing all the children CPU nodes + caches, or returns an error
    pub(super) fn parse_parent(
        mut parent: RawNode<'node>,
        phandles: &mut Map<u32, Rc<device::Node<'node>>>,
    ) -> Result<(CpuMap<'node>, CacheMap<'node>), RootError> {
        let (Ok(cpu_addr_cells), Ok(0)) = parent.extract_cell_counts() else {
            return Err(RootError::Reg);
        };
        let cpu_addr_cells = NonZeroU8::new(cpu_addr_cells).ok_or(RootError::Reg)?;

        let caches = parent
            .children
            .extract_if(|name, _| !name.node_name().starts_with(NodeNames::cpu_prefix()))
            .map(|(_, node)| {
                HigherLevel::new(node, phandles)
                    .map(|(phandle, cache)| (phandle, Rc::new(cache)))
                    .map_err(RootError::Cache)
            })
            .try_collect()?;

        parent
            .children
            .into_iter()
            .map(|(name, node)| {
                let node = Rc::new(
                    Self::new(node, &parent.properties, &caches, cpu_addr_cells)
                        .map_err(RootError::Cpu)?,
                );

                if name
                    .unit_address()
                    .is_some_and(|address| address != node.reg.into())
                {
                    return Err(RootError::RegMismatch(name.unit_address(), node.reg));
                }
                Ok((node.reg, node))
            })
            .try_collect()
            .map(|cpus| (cpus, caches))
    }

    #[must_use]
    #[inline]
    pub const fn enable_method(&self) -> Option<&EnableMethod<'_>> {
        self.enable_method.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn l1_cache(&self) -> &L1 {
        &self.l1_cache
    }

    #[must_use]
    #[inline]
    pub const fn status(&self) -> &Status {
        &self.status
    }

    #[must_use]
    #[inline]
    pub const fn next_cache(&self) -> Option<&Rc<HigherLevel<'_>>> {
        self.next_cache.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn properties(&self) -> &Map<&'node CStr, U32ByteSlice<'node>> {
        &self.properties
    }
}
