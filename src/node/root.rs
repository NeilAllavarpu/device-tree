//! The root node of the device tree. All nodes are descendants of this.

use super::chosen::{Chosen, Error};
use super::{cache::HigherLevel, cpu, memory_region, reserved_memory, RawNode, RawNodeError};
use super::{device, ChildMap, PropertyMap};
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
    reserved_memory: Option<Map<NameRef<'node>, reserved_memory::Node<'node>>>,
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
    aliases: Map<NameRef<'node>, Rc<device::Node<'node>>>,
    /// Map of phandles to nodes
    phandles: Map<u32, Rc<device::Node<'node>>>,
    /// The remainder of this node's properties
    properties: PropertyMap<'node>,
    /// Children nodes of the root
    children: ChildMap<'node>,
    /// Runtime parameters
    chosen: Option<Chosen<'node>>,
}

impl<'node> Node<'node> {
    /// Returns the map of CPUs described by the device tree
    #[must_use]
    #[inline]
    pub const fn cpus(&self) -> &Map<u32, Rc<cpu::Node<'node>>> {
        &self.cpus
    }

    #[must_use]
    #[inline]
    pub const fn model(&self) -> &Model<'node> {
        &self.model
    }

    #[must_use]
    #[inline]
    pub const fn compatible(&self) -> &[Model<'_>] {
        &self.compatible
    }

    #[must_use]
    #[inline]
    pub const fn serial_number(&self) -> Option<&CStr> {
        self.serial_number
    }

    #[must_use]
    #[inline]
    pub const fn chassis(&self) -> Option<&ChassisType> {
        self.chassis.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn higher_caches(&self) -> &Map<u32, Rc<HigherLevel<'node>>> {
        &self.higher_caches
    }

    #[must_use]
    #[inline]
    pub const fn memory(&self) -> &[MemoryRegion<'_>] {
        &self.memory
    }

    #[must_use]
    #[inline]
    pub const fn phandles(&self) -> &Map<u32, Rc<device::Node<'node>>> {
        &self.phandles
    }

    #[must_use]
    #[inline]
    pub const fn chosen(&self) -> Option<&Chosen<'node>> {
        self.chosen.as_ref()
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
    /// The parent node for CPU nodes is missing
    CpuRoot,
    /// Parsing CPUs failed
    Cpu(cpu::RootError),
    /// Matching a reg field to the unit name failed
    RegMismatch(Option<u64>, u64),
    /// A cache node is invalid
    Cache,
    /// Parsing regs or ranges from address cells or size cells failed
    Cells(CellError),
    /// The parent node for reserved memory is invalid
    ReservedMemoryRoot,
    /// A reserved memory node is invalid
    ReservedMemory(reserved_memory::RootError),
    /// A memory region is invalid
    Memory(memory_region::Error),
    /// The type of a node was invalid
    Type,
    Child(device::Error),
    Chassis(ChassisError<'node>),
    Alias,
    Chosen(Error<'node>),
}

/// "Constants" for various node names
pub(super) struct NodeNames;

impl NodeNames {
    /// The node name for the CPUs parent node
    fn cpus() -> NameRef<'static> {
        NameRef::try_from(b"cpus".as_slice()).expect("Should be a valid name")
    }

    /// The prefix for CPU nodes' names
    pub(super) fn cpu_prefix() -> &'static NameSlice {
        <&NameSlice>::try_from(b"cpu".as_slice()).expect("Should be a valid name")
    }
    /// The node name for memory nodes
    fn memory() -> &'static NameSlice {
        <&NameSlice>::try_from(b"memory".as_slice()).expect("Should be a valid name")
    }

    /// The node name for reserved memory nodes
    pub(super) fn reserved_memory() -> NameRef<'static> {
        NameRef::try_from(b"reserved-memory".as_slice()).expect("Should be a valid name")
    }

    /// The node name for the alias node
    fn aliases() -> NameRef<'static> {
        b"aliases"
            .as_slice()
            .try_into()
            .expect("Should be a valid name")
    }

    /// The node name for the chosen node
    fn chosen() -> NameRef<'static> {
        b"chosen"
            .as_slice()
            .try_into()
            .expect("Should be a valid name")
    }

    #[cfg(feature = "rpi")]
    /// The node name for the symbols node
    fn symbols() -> NameRef<'static> {
        b"__symbols__"
            .as_slice()
            .try_into()
            .expect("Should be a valid name")
    }
}

/// Parses the root `/aliases` node and returns a map that converts a name into a reference to the resolved node
fn parse_aliases<'data, 'root>(
    aliases_node: Option<RawNode<'data>>,
    root: &'root Node<'data>,
) -> Map<NameRef<'data>, Rc<device::Node<'data>>>
where
    'data: 'root,
{
    aliases_node.map_or_else(Map::new, |aliases| {
        aliases
            .properties
            .into_iter()
            .filter_map(|(name, path)| {
                NameRef::try_from(name.to_bytes()).ok().zip(
                    CStr::from_bytes_until_nul(path.into())
                        .ok()
                        .and_then(|c_path| {
                            use super::Node;
                            let entry: Option<Rc<device::Node<'data>>> =
                                root.find_str(c_path.to_bytes());
                            if entry.is_none() {
                                eprintln!(
                                    "WARNING: Could not match {} to {}",
                                    name.to_string_lossy(),
                                    c_path.to_string_lossy()
                                );
                            }
                            entry
                        }),
                )
            })
            .collect()
    })
}

impl<'data> super::Node<'data> for Node<'data> {
    #[inline]
    fn properties(&self) -> &PropertyMap {
        &self.properties
    }

    #[inline]
    fn children(&self) -> &ChildMap<'data> {
        &self.children
    }

    #[inline]
    fn find<'node, 'path>(
        &'node self,
        direct_child_name: NameRef<'path>,
        mut rest_path: impl Iterator<Item = NameRef<'path>>,
    ) -> Option<Rc<device::Node<'data>>>
    where
        'path: 'data,
    {
        let grandchild_name_opt = rest_path.next();
        if let Some(node) = self.aliases.get(&direct_child_name) {
            return grandchild_name_opt.map_or(Some(Rc::clone(node)), |grandchild_name| {
                node.find(grandchild_name, rest_path)
            });
        }
        let entry = if direct_child_name == NodeNames::reserved_memory() {
            let reserved_memory = self.reserved_memory.as_ref()?;
            grandchild_name_opt
                .and_then(|grandchild_name| {
                    reserved_memory.get(&grandchild_name).and_then(|grandchild| {
                        rest_path.next().map_or_else(|| {
                            eprintln!("WARNING: References to non-plain device nodes are not currently supported: /{direct_child_name}/{grandchild_name}");
                            None
                        }, |great_grandchild_name| {
                            grandchild.find(great_grandchild_name, rest_path)
                        })
                    })
                })
        } else {
            self.children
                .get(&direct_child_name)
                .and_then(|direct_child| {
                    grandchild_name_opt.map_or(Some(Rc::clone(direct_child)), |grandchild_name| {
                        direct_child.find(grandchild_name, rest_path)
                    })
                })
        };
        entry
    }
}

impl<'node> TryFrom<RawNode<'node>> for Node<'node> {
    type Error = NodeError<'node>;

    #[inline]
    #[expect(clippy::too_many_lines)]
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

        let (cpus, caches) = cpu::Node::parse_parent(
            value
                .children
                .remove(&NodeNames::cpus())
                .ok_or(NodeError::CpuRoot)?,
            &mut phandles,
        )
        .map_err(NodeError::Cpu)?;

        let reserved_memory = value
            .children
            .remove(&NodeNames::reserved_memory())
            .map(|reserved_root| {
                reserved_memory::Node::parse_parent(
                    reserved_root,
                    address_cells,
                    size_cells,
                    &mut phandles,
                )
                .map_err(NodeError::ReservedMemory)
            })
            .transpose()?;

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
        #[cfg(feature = "rpi")]
        let symbols_node = value.children.remove(&NodeNames::symbols());

        let chosen_node = value.children.remove(&NodeNames::chosen());

        let (properties, children) = value.into_components(&mut phandles, None);
        let children: Map<NameRef<'node>, Rc<device::Node<'node>>> = match children {
            Ok(children) => children,
            Err(RawNodeError::Cells) => return Err(NodeError::Cells(CellError::Invalid)),
            Err(RawNodeError::Child(child)) => return Err(NodeError::Child(child)),
        };

        let mut root = Self {
            phandles,
            aliases: Map::default(),
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
            chosen: Option::default(),
        };

        root.aliases = parse_aliases(aliases_node, &root);
        #[cfg(feature = "rpi")]
        root.aliases.extend(parse_aliases(symbols_node, &root));

        root.chosen = chosen_node
            .map(|chosen| Chosen::from_node(chosen, &root).map_err(NodeError::Chosen))
            .transpose()?;

        Ok(root)
    }
}
