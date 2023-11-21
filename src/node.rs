use crate::map::Map;
use crate::node_name::NameRef;

use crate::parse::U32ByteSlice;
use crate::property::parse_model_list;
use crate::property::to_c_str;
use crate::property::Model;
use crate::property::Range;
use crate::property::Status;
use alloc::boxed::Box;
use core::ffi::CStr;

pub mod cache;
pub mod cpu;
pub mod memory_region;
pub mod reserved_memory;
pub mod root;

/// Namespace of constants for various property keys to look up
#[expect(clippy::exhaustive_structs, reason = "No fields exported")]
pub struct PropertyKeys;

impl PropertyKeys {
    pub const ADDRESS_CELLS: &'static CStr = to_c_str(b"#address-cells\0");
    pub const SIZE_CELLS: &'static CStr = to_c_str(b"#size-cells\0");
    pub const REG: &'static CStr = to_c_str(b"reg\0");
    pub const RANGES: &'static CStr = to_c_str(b"ranges\0");
    pub const COMPATIBLE: &'static CStr = to_c_str(b"compatible\0");
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
}

/// A Device Tree Node
#[derive(Debug)]
pub(crate) struct RawNode<'node> {
    pub(crate) children: Map<NameRef<'node>, RawNode<'node>>,
    pub(crate) properties: Map<&'node CStr, U32ByteSlice<'node>>,
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

pub enum RawNodeError {
    Cells,
    Child(Error),
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
    ) -> (
        Map<&'node CStr, U32ByteSlice<'node>>,
        Result<Map<NameRef<'node>, DeviceNode<'node>>, RawNodeError>,
    ) {
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
                        DeviceNode::new(raw_node, child_addr_cells.ok(), child_size_cells.ok())
                            .map(|device_node| (name, device_node))
                    })
                    .try_collect()
                    .map_err(RawNodeError::Child)
            },
        )
    }
}

pub trait Node {
    fn properties(&self) -> &Map<&CStr, U32ByteSlice>;
    fn children(&self) -> &Map<NameRef, &DeviceNode>;
}

/// A Device Tree Node
#[derive(Debug)]
pub struct DeviceNode<'node> {
    /// Children
    pub(crate) children: Map<NameRef<'node>, DeviceNode<'node>>,
    pub(crate) compatible: Option<Box<[Model]>>,
    pub(crate) model: Option<Model>,
    pub(crate) reg: Option<Box<[[u64; 2]]>>,
    pub(crate) ranges: Option<Box<[Range]>>,
    pub(crate) status: Status<'node>,
    pub(crate) properties: Map<&'node CStr, U32ByteSlice<'node>>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Reg,
    Compatible,
    Model,
    Ranges,
    Status,
    Cells,
    Child(Box<Error>),
}

impl<'node> DeviceNode<'node> {
    ///c
    fn new(
        mut value: RawNode<'node>,
        address_cells: Option<u8>,
        size_cells: Option<u8>,
    ) -> Result<Self, Error> {
        let (child_address_cells, child_size_cells) = value.extract_cell_counts();
        let reg = value
            .properties
            .remove(&PropertyKeys::REG)
            .map(|bytes| {
                address_cells
                    .zip(size_cells)
                    .and_then(|cells| bytes.into_cells_slice(&cells.into()))
                    .ok_or(Error::Reg)
            })
            .transpose()?;
        let compatible = value
            .properties
            .remove(&PropertyKeys::COMPATIBLE)
            .map(|bytes| parse_model_list(bytes.into()).map_err(|_err| Error::Compatible))
            .transpose()?;
        let model = value
            .properties
            .remove(&PropertyKeys::MODEL)
            .map(|bytes| Model::try_from(<&[u8]>::from(bytes)).map_err(|_| Error::Model))
            .transpose()?;

        let ranges = value
            .properties
            .remove(&PropertyKeys::RANGES)
            .map(|bytes| {
                child_address_cells
                    .ok()
                    .zip(address_cells)
                    .zip(child_size_cells.ok())
                    .and_then(|((child_address_cells, address_cells), child_size_cells)| {
                        bytes
                            .into_cells_slice(&[
                                child_address_cells,
                                address_cells,
                                child_size_cells,
                            ])
                            .map(|entries| {
                                entries.iter().map(|&range| Range::from(range)).collect()
                            })
                    })
                    .ok_or(Error::Ranges)
            })
            .transpose()?;
        let status = value
            .properties
            .remove(&PropertyKeys::STATUS)
            .map_or(Ok(Status::Ok), |bytes| {
                Status::try_from(<&[u8]>::from(bytes)).map_err(|_err| Error::Status)
            })?;

        let (properties, children) = value.into_components();
        let children = match children {
            Ok(children) => children,
            Err(RawNodeError::Cells) => return Err(Error::Cells),
            Err(RawNodeError::Child(child)) => return Err(Error::Child(Box::new(child))),
        };
        Ok(Self {
            children,
            compatible,
            model,
            reg,
            ranges,
            status,
            properties,
        })
    }
}

#[derive(Debug)]
pub enum ManuModel {
    ManuModel(Box<str>, Box<str>),
    Other(Box<str>),
}
#[derive(Debug)]

pub enum ChassisType {
    Desktop,
}
