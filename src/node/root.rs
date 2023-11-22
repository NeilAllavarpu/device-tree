//! The root node of the device tree. All nodes are descendants of this.

use super::{cache::HigherLevel, cpu, memory_region, reserved_memory, RawNode, RawNodeError};
use super::{ChildMap, DeviceNode, PropertyMap};
use crate::property::{ChassisError, ChassisType};
use crate::{
    map::Map,
    node::{memory_region::MemoryRegion, CellError, PropertyKeys},
    node_name::{NameRef, NameSlice},
    property::Model,
};
use alloc::rc::Rc;
use core::ffi::CStr;
use core::num::NonZeroU8;

/// The base of the device tree that all nodes are children of
#[derive(Debug)]
pub struct Node<'node> {
    /// Specifies a string that uniquely identifies the model of the system board.
    /// The recommended format is `“manufacturer,model-number”``.
    model: Model<'node>,
    /// Specifies a list of platform architectures with which this platform is compatible.
    /// This property can be used by operating systems in selecting platform specific code.
    /// The recommended form of the property value is: `"manufacturer,model"`. For example: `compatible = "fsl,mpc8572ds"`
    compatible: Box<[Model<'node>]>,
    /// Specifies a string representing the device’s serial number.
    serial_number: Option<&'node CStr>,
    /// Specifies a string that identifies the form-factor of the system.
    chassis: Option<ChassisType>,
    /// Higher level caches present in the processor, beyond the L1
    higher_caches: Map<u32, Rc<HigherLevel<'node>>>,
    /// Reserved memory is specified as a node under the /reserved-memory node.
    /// The operating system shall exclude reserved memory from normal usage.
    /// One can create child nodes describing particular reserved (excluded from normal use) memory regions.
    /// Such memory regions are usually designed for the special usage by various device drivers.
    reserved_memory: Map<NameRef<'node>, reserved_memory::Node<'node>>,
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
    cpus: Map<u32, Rc<cpu::Node<'node>>>,
    /// Each property of the `/aliases` node defines an alias.
    /// The property name specifies the alias name.
    /// The property value specifies the full path to a node in the devicetree.
    /// For example, the property `serial0 = "/simple-bus@fe000000/ serial@llc500"` defines the alias `serial0`.
    aliases: Map<NameRef<'node>, Rc<DeviceNode<'node>>>,
    /// Map of phandles to nodes
    phandles: Map<u32, Rc<DeviceNode<'node>>>,
    /// The remainder of this node's properties
    properties: PropertyMap<'node>,
    /// Children nodes of the root
    children: ChildMap<'node>,
}

impl<'node> Node<'node> {
    /// Returns the map of CPUs described by the device tree
    #[must_use]
    #[inline]
    pub const fn cpus(&self) -> &Map<u32, Rc<cpu::Node<'node>>> {
        &self.cpus
    }
}

/// Errors from parsing a root node
#[derive(Debug)]
#[non_exhaustive]
pub enum NodeError<'node> {
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
    Chassis(ChassisError<'node>),
    Alias,
}

/// "Constants" for various node names
struct NodeNames;

impl NodeNames {
    /// The node name for the CPUs parent node
    fn cpus() -> NameRef<'static> {
        NameRef::try_from(b"cpus".as_slice()).expect("Should be a valid name")
    }

    /// The prefix for CPU nodes' names
    fn cpu_prefix() -> &'static NameSlice {
        <&NameSlice>::try_from(b"cpu".as_slice()).expect("Should be a valid name")
    }
    /// The node name for memory nodes
    fn memory() -> &'static NameSlice {
        <&NameSlice>::try_from(b"memory".as_slice()).expect("Should be a valid name")
    }

    /// The node name for reserved memory nodes
    fn reserved_memory() -> NameRef<'static> {
        NameRef::try_from(b"reserved-memory".as_slice()).expect("Should be a valid name")
    }

    /// The node names for the alias node
    fn aliases() -> NameRef<'static> {
        b"aliases"
            .as_slice()
            .try_into()
            .expect("Should be a valid name")
    }
}

impl<'node> TryFrom<RawNode<'node>> for Node<'node> {
    type Error = NodeError<'node>;

    #[inline]
    fn try_from(mut value: RawNode<'node>) -> Result<Self, Self::Error> {
        let mut phandles = Map::new();
        let model = value
            .properties
            .remove(PropertyKeys::MODEL)
            .and_then(|bytes| <&CStr>::try_from(bytes).ok())
            .map(Model::from)
            .ok_or(NodeError::Model)?;

        let compatible = value
            .properties
            .remove(&PropertyKeys::COMPATIBLE)
            .and_then(|compatible| compatible.try_into().ok())
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
                HigherLevel::new(node, &mut phandles)
                    .map(|(phandle, cache)| (phandle, Rc::new(cache)))
                    .map_err(|_err| NodeError::Cache)
            })
            .try_collect()?;

        let cpus = cpus_root
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

        let reserved_memory: Map<_, _> = reserved_memory_root
            .children
            .into_iter()
            .map(|(name, node)| {
                reserved_memory::Node::new(node, address_cells, size_cells, &mut phandles)
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

        let chassis = value
            .properties
            .remove(PropertyKeys::CHASSIS)
            .map(ChassisType::try_from)
            .transpose()
            .map_err(NodeError::Chassis)?;

        let aliases_node = value.children.remove(&NodeNames::aliases());

        let (properties, children) = value.into_components(&mut phandles);
        let children: Map<NameRef<'node>, Rc<DeviceNode<'node>>> = match children {
            Ok(children) => children,
            Err(RawNodeError::Cells) => return Err(NodeError::Cells(CellError::Invalid)),
            Err(RawNodeError::Child(child)) => return Err(NodeError::Child(child)),
        };

        let aliases = aliases_node.map_or_else(Map::new, |aliases| {
            aliases
                .properties
                .into_iter()
                .filter_map(|(name, path)| {
                    // println!("{name:?}: {path:?}");
                    NameRef::try_from(name.to_bytes()).ok().zip(
                        CStr::from_bytes_until_nul(path.into())
                            .ok()
                            .and_then(|c_path| {
                                use crate::node::Node;
                                let mut names = c_path
                                    .to_bytes()
                                    .split(|&char| char == b'/')
                                    .filter(|x| !x.is_empty())
                                    .filter_map(|x| NameRef::try_from(x).ok());
                                let direct_child_name = names.next()?;

                                let grandchild_name_opt = names
                                .next();

                                let entry = if direct_child_name == NodeNames::reserved_memory() {
                                    grandchild_name_opt
                                        .and_then(|grandchild_name| {
                                            reserved_memory.get(&grandchild_name)
                                        })
                                        .and_then(|grandchild| {
                                            names.next().map_or_else(|| {
                                                eprintln!("WARNING: References to non-plain device nodes are not currently supported: {}", c_path.to_string_lossy());
                                                None
                                            }, |great_grandchild_name| {
                                                grandchild.find(great_grandchild_name, names)
                                            })
                                        })
                                } else {
                                    children.get(&direct_child_name).and_then(|direct_child| {
                                        grandchild_name_opt.map_or(Some(direct_child), |grandchild_name| {
                                            direct_child.find(grandchild_name, names)
                                        })
                                    })
                                };
                                let entry = entry.map(|rc| {
                                    // This unsafe code bypasses the lifetime limitations of `DeviceNode` and the original `children` map not living long enough before the function returns
                                    let rc_pointer = Rc::into_raw(Rc::clone(rc));
                                    let rc_pointer = rc_pointer.cast::<DeviceNode<'node>>();
                                    // SAFETY:
                                    // * This raw pointer came from the `Rc::into_raw` call above
                                    // * `DeviceNode` always has the same size and alignment of itself
                                    // * It is valid to perform this semi-transmute because the lifetime of all `DeviceNode`s are tied to the lifetime of the underlying bytes of the device tree blob itself, and this `Root`` also cannot last longer than that.
                                    // So the lifetime of the new `DeviceNode` cannot outlive the data that it borrows from
                                    unsafe { Rc::from_raw(rc_pointer) }
                                });
                                if entry.is_none() {
                                    eprintln!("WARNING: Could not match {} to {}", name.to_string_lossy(), c_path.to_string_lossy());
                                }
                                entry
                            }),
                    )
                })
                .collect()
        });

        Ok(Self {
            phandles,
            aliases,
            model,
            compatible,
            serial_number,
            chassis,
            cpus,
            memory,
            reserved_memory,
            higher_caches: caches,
            properties,
            children,
        })
    }
}
