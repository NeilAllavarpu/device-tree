//! The root node of the device tree. All nodes are descendants of this.

use super::{
    cache::HigherLevel, cpu, memory_region, reserved_memory, ChassisType, DeviceNode, RawNode,
    RawNodeError,
};
use crate::{
    map::Map,
    node::{memory_region::MemoryRegion, CellError, PropertyKeys},
    node_name::{NameRef, NameSlice},
    parse::U32ByteSlice,
    property::{parse_model_list, Model},
};
use alloc::rc::Rc;
use core::ffi::CStr;
use core::num::NonZeroU8;

/// The base of the device tree that all nodes are children of
#[derive(Debug)]
pub struct Node<'node> {
    /// Specifies a string that uniquely identifies the model of the system board.
    /// The recommended format is `“manufacturer,model-number”``.
    model: Model,
    /// Specifies a list of platform architectures with which this platform is compatible.
    /// This property can be used by operating systems in selecting platform specific code.
    /// The recommended form of the property value is: `"manufacturer,model"`. For example: `compatible = "fsl,mpc8572ds"`
    compatible: Box<[Model]>,
    /// Specifies a string representing the device’s serial number.
    serial_number: Option<&'node CStr>,
    /// Specifies a string that identifies the form-factor of the system.
    chassis_type: Option<ChassisType>,
    /// Higher level caches present in the processor, beyond the L1
    pub higher_caches: Map<u32, Rc<HigherLevel<'node>>>,
    /// Reserved memory is specified as a node under the /reserved-memory node.
    /// The operating system shall exclude reserved memory from normal usage.
    /// One can create child nodes describing particular reserved (excluded from normal use) memory regions.
    /// Such memory regions are usually designed for the special usage by various device drivers.
    pub(crate) reserved_memory: Map<NameRef<'node>, reserved_memory::Node<'node>>,
    /// A memory device node is required for all devicetrees and describes the physical memory layout for the system.
    /// If a system has multiple ranges of memory, multiple memory nodes can be created, or the ranges can be specified in the reg property of a single memory node.
    ///
    /// The client program may access memory not covered by any memory reservations using any storage attributes it chooses.
    /// However, before changing the storage attributes used to access a real page, the client program is responsible for performing actions required by the architecture and implementation, possibly including flushing the real page from the caches. The boot program is responsible for ensuring that, without taking any action associated with a change in storage attributes, the client program can safely access all memory (including memory covered by memory reservations) as WIMG = 0b001x. That is:
    /// * not Write Through Required
    /// * not Caching Inhibited
    /// * Memory Coherence
    /// * Required either not Guarded or Guarded
    ///
    /// If the VLE storage attribute is supported, with VLE=0.
    memory: Box<[MemoryRegion<'node>]>,
    /// Child cpu nodes which represent the system's CPUs.
    pub cpus: Map<u32, Rc<cpu::Node<'node>>>,
    /// The remainder of this node
    properties: Map<&'node CStr, U32ByteSlice<'node>>,
    children: Map<NameRef<'node>, DeviceNode<'node>>,
}

/// Errors from parsing a root node
#[derive(Debug)]
#[non_exhaustive]
pub enum NodeError {
    /// The model is missing or invalid
    Model,
    /// The compatible field is missing or invalid
    Compatible,
    /// The serial number is invalid
    SerialNumber,
    /// The parent node for CPU nodes is invalid
    CpuRoot,
    /// A CPU node is invalid
    Cpu(cpu::NodeError),
    /// Matching a reg field to the unit name failed
    RegMismatch(Option<u64>, u64),
    /// A cache node is invalid
    Cache,
    /// Parsing regs or ranges from address cells or size cells failed
    Cells(CellError),
    /// The parent node for reserved memory is invalid
    ReservedMemoryRoot,
    /// A reserved memory node is invalid
    ReservedMemory(reserved_memory::Error),
    /// A memory region is invalid
    Memory(memory_region::Error),
    /// The type of a node was invalid
    Type,
    Child(super::Error),
}

/// "Constants" for various node names
struct NodeNames;

impl NodeNames {
    /// The node name for the CPUs parent node
    pub fn cpus() -> NameRef<'static> {
        NameRef::try_from(b"cpus".as_slice()).expect("Should be a valid name")
    }

    /// The prefix for CPU nodes' names
    pub fn cpu_prefix() -> &'static NameSlice {
        <&NameSlice>::try_from(b"cpu".as_slice()).expect("Should be a valid name")
    }
    /// The node name for memory nodes
    pub fn memory() -> &'static NameSlice {
        <&NameSlice>::try_from(b"memory".as_slice()).expect("Should be a valid name")
    }

    /// The node name for reserved memory nodes
    pub fn reserved_memory() -> NameRef<'static> {
        NameRef::try_from(b"reserved-memory".as_slice()).expect("Should be a valid name")
    }
}

impl<'node> TryFrom<RawNode<'node>> for Node<'node> {
    type Error = NodeError;

    #[inline]
    #[allow(clippy::too_many_lines)]
    fn try_from(mut value: RawNode<'node>) -> Result<Self, Self::Error> {
        let model = value
            .properties
            .remove(PropertyKeys::MODEL)
            .and_then(|x| Model::try_from(<&[u8]>::from(x)).ok())
            .ok_or(NodeError::Model)?;

        let compatible = value
            .properties
            .remove(&PropertyKeys::COMPATIBLE)
            .and_then(|compatible| parse_model_list(compatible.into()).ok())
            .ok_or(NodeError::Model)?;

        let serial_number = value
            .properties
            .remove(&PropertyKeys::SERIAL_NUMBER)
            .map(|serial_number| {
                <&CStr>::try_from(serial_number).map_err(|_err| NodeError::SerialNumber)
            })
            .transpose()?;

        let (address_cells, size_cells) = value.extract_cell_counts();
        let (address_cells, size_cells) = (
            address_cells.map_err(NodeError::Cells)?,
            size_cells.map_err(NodeError::Cells)?,
        );
        let size_cells = NonZeroU8::new(size_cells).ok_or(NodeError::Cells(CellError::Invalid))?;

        let mut cpus_root = value
            .children
            .remove(&NodeNames::cpus())
            .ok_or(NodeError::CpuRoot)?;

        let (Ok(cpu_addr_cells), Ok(0)) = cpus_root.extract_cell_counts() else {
            return Err(NodeError::CpuRoot);
        };
        let cpu_addr_cells = NonZeroU8::new(cpu_addr_cells).ok_or(NodeError::CpuRoot)?;

        let caches = cpus_root
            .children
            .extract_if(|name, _| !name.node_name().starts_with(NodeNames::cpu_prefix()))
            .map(|(_, node)| {
                HigherLevel::new(node, cpu_addr_cells.get())
                    .map(|(phandle, cache)| (phandle, Rc::new(cache)))
                    .map_err(|_err| NodeError::Cache)
            })
            .try_collect()?;

        let cpus: Map<_, _> = cpus_root
            .children
            .into_iter()
            .map(|(name, node)| {
                let node = Rc::new(
                    cpu::Node::new(node, &cpus_root.properties, &caches, cpu_addr_cells)
                        .map_err(NodeError::Cpu)?,
                );

                if name
                    .unit_address()
                    .is_some_and(|address| address != node.reg.into())
                {
                    return Err(NodeError::RegMismatch(name.unit_address(), node.reg.into()));
                }
                Ok((node.reg, node))
            })
            .try_collect()?;

        let mut reserved_memory_root = value
            .children
            .remove(&NodeNames::reserved_memory())
            .ok_or(NodeError::ReservedMemoryRoot)?;

        // #address-cells and #size-cells should use the same values as for the root node, and ranges should be empty so that address translation logic works correctly.
        let (reserved_memory_addr_cells, reserved_memory_size_cells) =
            reserved_memory_root.extract_cell_counts();

        if !(reserved_memory_addr_cells.is_ok_and(|cells| cells == address_cells)
            && reserved_memory_size_cells.is_ok_and(|cells| cells == size_cells.get())
            && reserved_memory_root
                .properties
                .remove(PropertyKeys::RANGES)
                .is_some_and(|ranges| ranges.is_empty()))
        {
            return Err(NodeError::ReservedMemoryRoot);
        }

        let reserved_memory = reserved_memory_root
            .children
            .into_iter()
            .map(|(name, node)| {
                reserved_memory::Node::new(node, address_cells, size_cells)
                    .map(|reserved_node| (name, reserved_node))
            })
            .try_collect()
            .map_err(NodeError::ReservedMemory)?;

        let memory = value
            .children
            .extract_if(|name, _| name.node_name() == NodeNames::memory())
            .map(|(name, memory_node)| {
                MemoryRegion::new(memory_node, &name, address_cells, size_cells.get())
                    .map_err(NodeError::Memory)
            })
            .try_collect()?;

        let (properties, children) = value.into_components();
        let children = match children {
            Ok(children) => children,
            Err(RawNodeError::Cells) => return Err(NodeError::Cells(CellError::Invalid)),
            Err(RawNodeError::Child(child)) => return Err(NodeError::Child(child)),
        };

        Ok(Self {
            model,
            compatible,
            serial_number,
            chassis_type: None,
            cpus,
            memory,
            reserved_memory,
            higher_caches: caches,
            properties,
            children,
        })
    }
}
