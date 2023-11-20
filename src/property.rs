//! Information about various features of the machine

use alloc::{boxed::Box, vec::Vec};
use core::fmt::Debug;

use core::ffi::CStr;

use crate::{
    map::Map,
    node::PropertyKeys,
    parse::{self, ParseStrError, U32ByteSlice},
};

/// A basic, fixed string
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]

pub(crate) struct String(pub(crate) Box<str>);

impl TryFrom<&[u8]> for String {
    type Error = ParseStrError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        parse::parse_str(value).map(|string| Self(string.into()))
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Model {
    ManufacturerModel(Box<str>, Box<str>),
    Other(Box<str>),
}

impl TryFrom<&[u8]> for Model {
    type Error = ParseStrError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let string = parse::parse_str(value)?;

        let mut parts = string.split(',');
        Ok(
            if let Some(manufacturer) = parts.next()
                && let Some(model) = parts.next()
                && let None = parts.next()
            {
                Self::ManufacturerModel(manufacturer.into(), model.into())
            } else {
                Self::Other(string.into())
            },
        )
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Range {
    pub(crate) child_address: u64,
    pub(crate) parent_address: u64,
    pub(crate) size: u64,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]

pub(crate) struct Reg {
    pub(crate) address: u64,
    pub(crate) size: u64,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ModelList(pub(crate) Box<[Model]>);

pub fn parse_model_list(mut value: &[u8]) -> Result<Box<[Model]>, ParseStrError> {
    let mut models = Vec::new();
    while !value.is_empty() {
        let model = Model::try_from(value)?;
        let length = match &model {
            Model::ManufacturerModel(manufacturer, model) => manufacturer
                .len()
                .checked_add(2) // 1 for nul byte, 1 for comma
                .and_then(|length| length.checked_add(model.len()))
                .expect("The null byte should have already been found"),
            Model::Other(string) if string.is_empty() => {
                assert_eq!(value.take_first(), Some(&0));
                continue;
            }
            Model::Other(string) if !string.is_empty() => string
                .len()
                .checked_add(1)
                .expect("The null byte should have already been found"),
            Model::Other(_) => unreachable!(),
        };

        #[expect(clippy::expect_used)]
        value
            .take(..length)
            .expect("CStr should not go past the end of the slice");

        models.push(model);
    }
    Ok(models.into_boxed_slice())
}

/// A list of strings
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]

struct StringList(Box<[String]>);

impl TryFrom<&[u8]> for StringList {
    type Error = ParseStrError;

    #[expect(clippy::unwrap_in_result)]
    fn try_from(mut value: &[u8]) -> Result<Self, Self::Error> {
        let mut strings = Vec::new();
        while !value.is_empty() {
            let string = String::try_from(value)?;
            #[expect(clippy::expect_used)]
            value
                .take(
                    ..(string
                        .0
                        .len()
                        .checked_add(1)
                        .expect("The null byte should have already been found")),
                )
                .expect("CStr should not go past the end of the slice");
            strings.push(string);
        }
        Ok(Self(strings.into_boxed_slice()))
    }
}

/// A 32-bit integer
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]

pub(crate) struct U32(pub u32);

impl TryFrom<&[u8]> for U32 {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() == 4 {
            let mut bytes = [0; 4];
            bytes.copy_from_slice(value);
            Ok(Self(u32::from_be_bytes(bytes)))
        } else {
            Err(())
        }
    }
}

/// A 64-bit integer
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]

struct U64(u64);

impl TryFrom<&[u8]> for U64 {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() == 8 {
            let mut bytes = [0; 8];
            bytes.copy_from_slice(value);
            Ok(Self(u64::from_be_bytes(bytes)))
        } else {
            Err(())
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Status<'a> {
    Ok,
    Disabled,
    Reserved,
    Fail(&'a [u8]),
}

impl<'a> TryFrom<&'a [u8]> for Status<'a> {
    type Error = &'a [u8];

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        CStr::from_bytes_until_nul(value).map_or(Err(value), |string| {
            let string = string.to_bytes();
            string.strip_prefix(b"fail").map_or_else(
                || match string {
                    b"okay" => Ok(Self::Ok),
                    b"disabled" => Ok(Self::Disabled),
                    b"reserved" => Ok(Self::Reserved),
                    b"fail" => unreachable!("Any prefix of 'fail' should already be removed"),
                    other => Err(other),
                },
                |code| Ok(Self::Fail(code)),
            )
        })
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnableType<'a> {
    SpinTable(u64),
    VendorSpecific(&'a [u8], &'a [u8]),
}

pub enum EnableTypeError {
    NotPresent,
    NoReleaseAddr,
    Invalid,
}

impl<'prop> EnableType<'prop> {
    /// Extracts and parses `EnableType` from a map of properties, returning the type if valid
    pub(crate) fn extract_from_properties(
        properties: &mut Map<&'prop CStr, U32ByteSlice<'prop>>,
    ) -> Result<Self, EnableTypeError> {
        properties
            .remove(&PropertyKeys::ENABLE_METHOD)
            .ok_or(EnableTypeError::NotPresent)
            .and_then(|bytes| <&CStr>::try_from(bytes).map_err(|_| EnableTypeError::Invalid))
            .and_then(|method| match method.to_bytes() {
                b"spin-table" => Ok(EnableType::SpinTable(
                    properties
                        .remove(&PropertyKeys::CPU_RELEASE_ADDR)
                        .ok_or(EnableTypeError::NoReleaseAddr)?
                        .try_into()
                        .map_err(|_err| EnableTypeError::NoReleaseAddr)?,
                )),
                string => {
                    let mut chunks = string.split(|&character| character == b',');
                    let vendor = chunks.next().ok_or(EnableTypeError::Invalid)?;
                    let vendor_method = chunks.next().ok_or(EnableTypeError::Invalid)?;
                    if chunks.next().is_some() {
                        Err(EnableTypeError::Invalid)
                    } else {
                        Ok(Self::VendorSpecific(vendor, vendor_method))
                    }
                }
            })
    }
}
// impl TryFrom<&CStr> for EnableType<'_> {
//     type Error = ();

//     fn try_from(value: &CStr) -> Result<Self, Self::Error> {
//         let value = value.to_bytes();

//         if value == b"spin-table" {
//             Ok(Self::SpinTable(0))
//         } else {
//             let mut chunks = string.split(',');
//             let vendor = chunks.next().ok_or(())?;
//             let method = chunks.next().ok_or(())?;
//             if chunks.next().is_some() {
//                 Err(())
//             } else {
//                 Ok(Self::VendorSpecific(vendor.into(), method.into()))
//             }
//         }
//     }
// }

// type PHandle<'a, A: Allocator> = &'a Node<'a, A>;

/// Various properties that a `Node` may have.
///
/// Not all properties may be present in any given `Node`
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Property {
    /// The compatible property value consists of one or more strings that define the specific programming model for the device. This list of strings should be used by a client program for device driver selection. The property value consists of a concatenated list of null terminated strings, from most specific to most general. They allow a device to express its compatibility with a family of similar devices, potentially allowing a single device driver to match against several devices.
    ///
    /// The recommended format is "manufacturer,model", where manufacturer is a string describing the name of the manufacturer (such as a stock ticker symbol), and model specifies the model number.
    ///
    /// The compatible string should consist only of lowercase letters, digits and dashes, and should start with a letter.
    ///
    /// A single comma is typically only used following a vendor prefix. Underscores should not be used.
    ///
    /// Example:
    ///
    ///     compatible = "fsl,mpc8641", "ns16550";
    ///
    /// In this example, an operating system would first try to locate a device driver that supported fsl,mpc8641. If a
    /// driver was not found, it would then try to locate a driver that supported the more general ns16550 device type.
    // Compatible(ModelList),
    /// The model property value is a <string> that specifies the manufacturer’s model number of the device.
    ///
    /// The recommended format is: "manufacturer,model", where manufacturer is a string describing the
    /// name of the manufacturer (such as a stock ticker symbol), and model specifies the model number.
    ///
    /// Example:
    ///
    ///     model = "fsl,MPC8349EMITX";
    // Model(Model),
    /// The phandle property specifies a numerical identifier for a node that is unique within the devicetree. The phandle property value is used by other nodes that need to refer to the node associated with the property.
    PHandle(U32),
    /// The #address-cells and #size-cells properties may be used in any device node that has children in the devicetree hierarchy and describes how child device nodes should be addressed. The #address-cells property defines the number of <u32> cells used to encode the address field in a child node’s reg property. The #size-cells property defines the number of <u32> cells used to encode the size field in a child node’s reg property.
    ///
    /// The #address-cells and #size-cells properties are not inherited from ancestors in the devicetree. They shall be explicitly defined.
    ///
    /// A DTSpec-compliant boot program shall supply #address-cells and #size-cells on all nodes that have children.
    ///
    /// If missing, a client program should assume a default value of 2 for #address-cells, and a value of 1 for #size-cells.
    AddressCells(U32),
    SizeCells(U32),
    /// Specifies a string representing the device’s serial number.
    SerialNumber(String),
    /// A string that specifies the boot arguments for the client program. The value could potentially be a null string if no boot arguments are required.
    BootArgs(String),
    /// The device_type property was used in IEEE 1275 to describe the device’s FCode programming model. Because DTSpec does not have FCode, new use of the property is deprecated, and it should be included only on cpu and memory nodes for compatibility with IEEE 1275–derived devicetrees.
    DeviceType(String),
    /// The status property indicates the operational status of a device. The lack of a status property should be treated as if the property existed with the value of "okay".
    // Status(Status),
    /// Describes the method by which a CPU in a disabled state is enabled. This property is required for CPUs with a status property with a value of "disabled". The value consists of one or more strings that define the method to release this CPU. If a client program recognizes any of the methods, it may use it.
    // EnableMethod(EnableType),
    /// The cpu-release-addr property is required for cpu nodes that have an enable-method property value of "spin-table". The value specifies the physical address of a spin table entry that releases a secondary CPU from its spin loop.
    ReleaseAddr(U64),
    RegRaw(Box<[u8]>),
    Reg(Box<[u8]>),
    Range(Range),
    InterruptParent(U32),
    /// Fallthrough case for unhandled/nonstandard property types
    Other(Box<str>, Box<[u8]>),
}

macro_rules! matcher {
    ($switch: ident: $value: ident, $($name:expr => $t:expr),*) => {
        match $switch {
        $($name => $value.try_into().ok().map($t),)+
        _ => Some(Self::Other($switch.into(), $value.into())),
        }
    };
}

// struct

impl Property {
    /// Attempts to parse a name and value into a property according to its predefined type.
    ///
    /// Returns `None` if the value coercion fails
    pub(crate) fn from_name_and_value(name: &str, value: &[u8]) -> Option<Self> {
        // match name {
        // "compatible" => value.try_into().ok().map(Self::Compatible),
        matcher!(
            name: value,
            // "compatible" => Self::Compatible,
            // "model" => Self::Model,
            "phandle" => Self::PHandle,
            "#address-cells" => Self::AddressCells,
            "#size-cells" => Self::SizeCells,
            "serial-number" => Self::SerialNumber,
            "bootargs" => Self::BootArgs,
            "device_type" => Self::DeviceType,
            // "status" => Self::Status,
            // "enable-method" => Self::EnableMethod,
            "interrupt-parent" => Self::InterruptParent,
            "cpu-release-addr" => Self::ReleaseAddr
        )
        // _ => Some(Self::Other(name.into(), value.into())),
        // }
    }

    pub(crate) fn evaluate(mut properties: Vec<Self>) -> Box<[Self]> {
        properties.sort_unstable();
        properties.into_boxed_slice()
    }
}

// const fn const_unwrap<T, E>(value: Result<T, E>) -> T {
//     if let Ok(value) = value {
//         value
//     } else {
//         unreachable!()
//     }
// }

pub const fn to_c_str(string: &[u8]) -> &CStr {
    if let Ok(s) = CStr::from_bytes_with_nul(string) {
        s
    } else {
        unreachable!()
    }
}

pub(crate) struct PropertyMap<'a>(Map<Box<CStr>, U32ByteSlice<'a>>);

pub enum PropertyLookupError {
    InvalidType,
    NotPresent,
}

impl PropertyMap<'_> {
    pub(crate) fn address_cells(&self) -> Result<u64, PropertyLookupError> {
        self.0
            .get(to_c_str(b"#address-cells\0"))
            .ok_or(PropertyLookupError::NotPresent)
            .and_then(|x| {
                x.clone()
                    .try_into()
                    .map_err(|_| PropertyLookupError::InvalidType)
            })
    }

    pub fn get(&self, attribute: &CStr) -> Option<&U32ByteSlice> {
        self.0.get(attribute)
    }
}
