//! The various types of nodes possible. The generic type is a `DeviceNode`, used as a catch-all, and specific types are parsed into their respective structs. The `Node` trait provides a broad, but common, interface to allow interoperability of the different types of nodes.

use crate::map::Map;
use crate::node_name::NameRef;

use crate::parse::to_c_str;
use crate::parse::U32ByteSlice;
use alloc::rc::Rc;
use core::ffi::CStr;

pub mod cache;
pub mod chosen;
pub mod cpu;
pub mod device;
pub mod memory_region;
pub mod reserved_memory;
pub mod root;

/// Maps a name to a child node
type ChildMap<'node> = Map<NameRef<'node>, Rc<device::DeviceNode<'node>>>;
/// Maps a property string key to the corresponding raw bytes
type PropertyMap<'node> = Map<&'node CStr, U32ByteSlice<'node>>;

/// Namespace of constants for various property keys to look up
#[expect(clippy::exhaustive_structs, reason = "No fields exported")]
pub struct PropertyKeys;

impl PropertyKeys {
    pub const ADDRESS_CELLS: &'static CStr = to_c_str(b"#address-cells\0");
    pub const SIZE_CELLS: &'static CStr = to_c_str(b"#size-cells\0");
    pub const REG: &'static CStr = to_c_str(b"reg\0");
    pub const RANGES: &'static CStr = to_c_str(b"ranges\0");
    pub const COMPATIBLE: &'static CStr = to_c_str(b"compatible\0");
    pub const CHASSIS: &'static CStr = to_c_str(b"chassis-type\0");
    pub const MODEL: &'static CStr = to_c_str(b"model\0");
    pub const STATUS: &'static CStr = to_c_str(b"status\0");
    pub const DEVICE_TYPE: &'static CStr = to_c_str(b"device_type\0");
    pub const SERIAL_NUMBER: &'static CStr = to_c_str(b"serial-number\0");
    pub const REUSABLE: &'static CStr = to_c_str(b"reusable\0");
    pub const SIZE: &'static CStr = to_c_str(b"size\0");
    pub const ALIGNMENT: &'static CStr = to_c_str(b"alignment\0");
    pub const NO_MAP: &'static CStr = to_c_str(b"no-map\0");
    pub const ALLOC_RANGES: &'static CStr = to_c_str(b"alloc-ranges\0");
    pub const MEMORY: &'static CStr = to_c_str(b"memory\0");
    pub const HOTPLUGGABLE: &'static CStr = to_c_str(b"hotpluggable\0");
    pub const RESERVED_MEMORY: &'static CStr = to_c_str(b"reserved-memory\0");
    pub const PHANDLE: &'static CStr = to_c_str(b"phandle\0");
    pub const CACHE_LEVEL: &'static CStr = to_c_str(b"cache-level\0");
    pub const CPU_RELEASE_ADDR: &'static CStr = to_c_str(b"cpu-release-addr\0");
    pub const CACHE_UNIFIED: &'static CStr = to_c_str(b"cache-unified\0");
    pub const NEXT_LEVEL_CACHE: &'static CStr = to_c_str(b"next-level-cache\0");
    pub const ENABLE_METHOD: &'static CStr = to_c_str(b"enable-method\0");
    pub const BOOTARGS: &'static CStr = to_c_str(b"bootargs\0");
    pub const STDIN_PATH: &'static CStr = to_c_str(b"stdin-path\0");
    pub const STDOUT_PATH: &'static CStr = to_c_str(b"stdout-path\0");
}

/// A Device Tree Node
#[derive(Debug)]
pub(crate) struct RawNode<'node> {
    /// Unparsed children, mapped from name to raw node
    pub(crate) children: Map<NameRef<'node>, RawNode<'node>>,
    /// Unparsed properties
    pub(crate) properties: PropertyMap<'node>,
}

/// Errors from parsing the address and size cell count properties of a node
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum CellError {
    /// Cell field was not present as a property
    NotPresent,
    /// Cell field was not a valid `u32`
    Invalid,
}

/// Errors from attempting to convert a raw node's children into the appropriate device nodes
#[non_exhaustive]
pub enum RawNodeError {
    /// Either the `address-cells` or `size-cells` field was invalid or missing when required
    Cells,
    /// Error from parsing some child node
    Child(device::Error),
}

impl<'node> RawNode<'node> {
    /// Creates a node with the given name, children, and properties
    pub(crate) fn new(
        children: impl IntoIterator<Item = (NameRef<'node>, Self)>,
        properties: Map<&'node CStr, U32ByteSlice<'node>>,
    ) -> Self {
        Self {
            children: children.into_iter().collect(),
            properties,
        }
    }

    /// Removes and returns the address and size cells of this node, if present
    fn extract_cell_counts(&mut self) -> (Result<u8, CellError>, Result<u8, CellError>) {
        /// Type-proper function to consume a byte slice into a single u32
        fn parse_cells(bytes: U32ByteSlice<'_>) -> Result<u8, CellError> {
            u32::try_from(bytes)
                .ok()
                .and_then(|x| u8::try_from(x).ok())
                .ok_or(CellError::Invalid)
        }
        (
            self.properties
                .remove(&PropertyKeys::ADDRESS_CELLS)
                .map_or(Ok(2), parse_cells),
            self.properties
                .remove(&PropertyKeys::SIZE_CELLS)
                .map_or(Ok(1), parse_cells),
        )
    }

    /// Decomposes this raw node into a parsed map of `DeviceNode` children and map of properties.
    ///
    /// Error conditions indicate any errors with parsing some child of the node
    fn into_components(
        mut self,
        phandles: &mut Map<u32, Rc<device::DeviceNode<'node>>>,
    ) -> (PropertyMap<'node>, Result<ChildMap<'node>, RawNodeError>) {
        let (child_addr_cells, child_size_cells) = self.extract_cell_counts();
        (
            self.properties,
            if matches!(child_addr_cells, Err(CellError::Invalid))
                || matches!(child_size_cells, Err(CellError::Invalid))
            {
                Err(RawNodeError::Cells)
            } else {
                self.children
                    .into_iter()
                    .map(|(name, raw_node)| {
                        device::DeviceNode::new(
                            raw_node,
                            child_addr_cells.ok(),
                            child_size_cells.ok(),
                            phandles,
                        )
                        .map(|device_node| (name, device_node))
                    })
                    .try_collect()
                    .map_err(RawNodeError::Child)
            },
        )
    }

    /// Decomposes this raw node into a parsed map of `DeviceNode` children and map of properties.
    ///
    /// Error conditions indicate any errors with parsing some child of the node
    fn into_components_from_cells(
        self,
        address_cells: Option<u8>,
        size_cells: Option<u8>,
        phandles: &mut Map<u32, Rc<device::DeviceNode<'node>>>,
    ) -> (PropertyMap<'node>, Result<ChildMap<'node>, RawNodeError>) {
        (
            self.properties,
            self.children
                .into_iter()
                .map(|(name, raw_node)| {
                    device::DeviceNode::new(raw_node, address_cells, size_cells, phandles)
                        .map(|device_node| (name, device_node))
                })
                .try_collect()
                .map_err(RawNodeError::Child),
        )
    }
}

pub trait Node<'node> {
    fn properties(&self) -> &PropertyMap;
    fn children(&self) -> &ChildMap<'node>;

    #[inline]
    fn find<'path>(
        &'node self,
        sub_path: NameRef<'path>,
        mut rest_path: impl Iterator<Item = NameRef<'path>>,
    ) -> Option<Rc<device::DeviceNode<'node>>>
    where
        'path: 'node,
    {
        self.children().get(&sub_path).and_then(|node| {
            rest_path.next().map_or_else(
                || Some(Rc::clone(node)),
                |next_path| node.find(next_path, rest_path),
            )
        })
    }

    #[inline]
    fn find_str<'path>(&'node self, path: &'node [u8]) -> Option<Rc<device::DeviceNode<'node>>>
    where
        'path: 'node,
    {
        let mut names = path
            .split(|&char| char == b'/')
            .filter(|x| !x.is_empty())
            .map(|x| NameRef::try_from(x).unwrap());

        let direct_child_name = names.next()?;
        self.find(direct_child_name, names)
    }
}
