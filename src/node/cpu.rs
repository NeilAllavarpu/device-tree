//! A CPU node. CPU nodes describe the physical cores that are present.

use crate::{
    map::Map,
    parse::U32ByteSlice,
    property::{EnableType, EnableTypeError},
};
use alloc::rc::Rc;
use core::{ffi::CStr, num::NonZeroU8};

use super::{
    cache::{HigherLevel, L1},
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
    enable_method: Option<EnableType<'node>>,
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

impl<'node> Node<'node> {
    /// Parses and creates a CPU node from the provided informaiton
    pub(super) fn new<'parsing>(
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

        let enable_method = match EnableType::extract_from_properties(&mut value.properties) {
            Ok(method) => Some(method),
            Err(EnableTypeError::NotPresent) => None,
            Err(EnableTypeError::NoReleaseAddr) => return Err(NodeError::ReleaseAddr),
            Err(EnableTypeError::Invalid) => return Err(NodeError::EnableMethod),
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
}
