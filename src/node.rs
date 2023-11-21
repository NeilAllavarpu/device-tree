use crate::map::Map;
use crate::node_name::NameRef;
use crate::node_name::NameSlice;
use crate::parse::U32ByteSlice;
use crate::property::parse_model_list;
use crate::property::to_c_str;
use crate::property::Model;
use crate::property::Range;
use crate::property::Status;
use alloc::boxed::Box;
use alloc::rc::Rc;
use core::ascii;
use core::ffi::CStr;
use core::num::NonZeroU32;
use core::num::NonZeroU8;

pub mod cache;
pub mod cpu;
pub mod memory_region;
pub mod reserved_memory;
pub mod root;

/// A Device Tree Node
#[derive(Debug)]
pub(crate) struct RawNode<'a> {
    pub(crate) children: Map<NameRef<'a>, RawNode<'a>>,
    pub(crate) properties: Map<&'a CStr, U32ByteSlice<'a>>,
}

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

// const RESERVED_MEMORY_NODE: &'static NameRef

#[derive(Debug, Clone)]
pub enum CellError {
    NotPresent,
    Invalid,
}

impl<'a> RawNode<'a> {
    /// Creates a node with the given name, children, and properties
    pub(crate) fn new(
        children: impl IntoIterator<Item = (NameRef<'a>, Self)>,
        properties: Map<&'a CStr, U32ByteSlice<'a>>,
    ) -> Self {
        Self {
            children: children.into_iter().collect(),
            properties,
        }
    }

    /// Returns the address and size cells of this, if present
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
}

/// A Device Tree Node
#[derive(Debug)]
pub(crate) struct Node<'a> {
    pub(crate) children: Map<NameRef<'a>, Rc<Node<'a>>>,
    pub(crate) compatible: Option<Box<[Model]>>,
    pub(crate) model: Option<Model>,
    pub(crate) reg: Option<Box<[(u64, u64)]>>,
    pub(crate) ranges: Option<Box<[Range]>>,
    pub(crate) status: Status<'a>,
    pub(crate) other: Map<Box<CStr>, Box<[u32]>>,
}

fn parse_cells(bytes: &mut U32ByteSlice<'_>, address_cells: u8, size_cells: u8) -> (u64, u64) {
    let address = match address_cells {
        0 => unreachable!("Address cells should never be 0"),
        1 => bytes.consume_u32().map(u64::from),
        2 => bytes.consume_u64(),
        count => {
            let value = bytes.consume_u64();
            for _ in 2..count {
                if bytes.consume_u32() != Some(0) {
                    println!("Cannot handle address cell count {address_cells}");
                }
            }
            value
        }
    }
    .unwrap();

    let length = match size_cells {
        0 => Some(0),
        1 => bytes.consume_u32().map(u64::from),
        2 => bytes.consume_u64(),
        _ => unimplemented!("Cannot handle size cell count {size_cells}"),
    }
    .unwrap();

    (address, length)
}

impl<'a> Node<'a> {
    fn new(mut value: RawNode<'a>, address_cells: Option<u8>, size_cells: Option<u8>) -> Self {
        let (child_address_cells, child_size_cells) = value.extract_cell_counts();
        // let (child_address_cells, child_size_cells) =
        //     (child_address_cells.unwrap(), child_size_cells.unwrap());
        println!("node {value:#?}");

        Self {
            reg: value.properties.remove(&PropertyKeys::REG).map(|mut bytes| {
                let mut reg_list = Vec::with_capacity(
                    bytes.len() / usize::try_from(address_cells.unwrap() + size_cells.unwrap()).unwrap(),
                );
                while !bytes.is_empty() {
                    reg_list.push(parse_cells(&mut bytes, address_cells.unwrap(), size_cells.unwrap()))
                }
                reg_list.into_boxed_slice()
            }),
            compatible: value.properties.remove(&PropertyKeys::COMPATIBLE).map(|bytes| {
                parse_model_list(bytes.into()).unwrap()
                // let mut compatible_list = Vec::new();
                // let slice = <&[u8]>::from(bytes);
                // while !slice.is_empty() {
                //     let c_str = CStr::from_bytes_with_nul(slice).unwrap();
                //     compatible_list.push(Model::try_from(c_str.try_into().unwrap()).unwrap());
                //     slice.take(..c_str.len())
                // }
                // compatible_list.into_boxed_slice()
            }),
            model: value
                .properties
                .remove(&PropertyKeys::MODEL)
                .map(|bytes| Model::try_from(<&[u8]>::from(bytes)).unwrap()),
            ranges: value.properties.remove(&PropertyKeys::RANGES).map(|mut bytes| {
                let mut range_list = Vec::with_capacity(
                    bytes.len()
                        / usize::try_from(address_cells.unwrap() + size_cells.unwrap() + child_size_cells.clone().unwrap()).unwrap(),
                );
                while !bytes.is_empty() {
                    range_list.push(Range {
                        child_address: match child_address_cells.clone() {
                            Ok(0) => unreachable!("Address cells should never be 0"),
                            Ok(1) => bytes.consume_u32().map(u64::from),
                            Ok(2) => bytes.consume_u64(),
                            count => {
                                let v = bytes.consume_u64();
                                for _ in 2..count.unwrap() {
                                    if let Some(extra) = bytes.consume_u32() && extra != 0 {
                                        eprintln!(
                                            "Cannot handle address cell count {child_address_cells:?}: 0x{extra:X}"
                                        );
                                    }
                                }
                                v
                            }
                        }
                        .unwrap(),
                        parent_address: match address_cells.unwrap() {
                            0 => unreachable!("Address cells should never be 0"),
                            1 => bytes.consume_u32().map(u64::from),
                            2 => bytes.consume_u64(),
                            _ => unimplemented!("Cannot handle address cell count {address_cells:?}"),
                        }
                        .unwrap(),
                        size: match child_size_cells.clone() {
                            Ok(0) => unreachable!("Size of a range should never be 0"),
                            Ok(1) => bytes.consume_u32().map(u64::from),
                            Ok(2) => bytes.consume_u64(),
                            _ => unimplemented!("Cannot handle address cell count {address_cells:?}"),
                        }
                        .unwrap(),
                    })
                }
                range_list.into_boxed_slice()
            }),
            status: value
                .properties
                .remove(&PropertyKeys::STATUS)
                .map(|mut bytes| Status::try_from(<&[u8]>::from(bytes)).unwrap())
                .unwrap_or(Status::Ok),
            other: value
                .properties
                .into_iter()
                .map(|(label, bytes)| (label.into(), <&[u32]>::from(bytes).into()))
                .collect(),
                children: value
                    .children
                    .into_iter()
                    .map(|(name, raw_node)| {
                        (
                            name,
                            Rc::new(Node::new(raw_node, child_address_cells.clone().ok(), child_size_cells.clone().ok())),
                        )
                    })
                    .collect(),
        }
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
