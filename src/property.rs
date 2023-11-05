//! Information about various features of the machine

use alloc::{boxed::Box, vec::Vec};
use core::fmt::Debug;

use crate::parse::{self, ParseStrError};

/// A basic, fixed string
#[derive(Debug)]
struct String(Box<str>);

impl TryFrom<&[u8]> for String {
    type Error = ParseStrError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        parse::parse_str(value).map(|string| Self(string.into()))
    }
}

/// A list of strings
#[derive(Debug)]
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
#[derive(Debug)]
struct U32(u32);

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
#[derive(Debug)]
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

struct Empty();

#[derive(Debug)]
enum StatusType {
    Ok,
    Disabled,
    Reserved,
    Fail(Box<str>),
}

impl TryFrom<&[u8]> for StatusType {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match parse::parse_str(value) {
            Ok(string) => {
                if let Some(code) = string.strip_prefix("fail") {
                    Ok(Self::Fail(code.into()))
                } else {
                    match string {
                        "okay" => Ok(Self::Ok),
                        "disabled" => Ok(Self::Disabled),
                        "reserved" => Ok(Self::Reserved),
                        "fail" => unreachable!(),
                        _ => Err(()),
                    }
                }
            }
            Err(_) => Err(()),
        }
    }
}

#[derive(Debug)]
enum EnableType {
    SpinTable(u64),
    VendorSpecific(Box<str>, Box<str>),
}

impl TryFrom<&[u8]> for EnableType {
    type Error = ();

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match parse::parse_str(value) {
            Ok(string) => {
                if string == "spin-table" {
                    Ok(Self::SpinTable(0))
                } else {
                    let mut chunks = string.split(",");
                    let vendor = chunks.next().ok_or(())?;
                    let method = chunks.next().ok_or(())?;
                    if chunks.next().is_some() {
                        Err(())
                    } else {
                        Ok(Self::VendorSpecific(vendor.into(), method.into()))
                    }
                }
            }
            Err(_) => Err(()),
        }
    }
}

// type PHandle<'a, A: Allocator> = &'a Node<'a, A>;

/// Various properties that a `Node` may have.
///
/// Not all properties may be present in any given `Node`
#[derive(Debug)]
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
    Compatible(StringList),
    /// The model property value is a <string> that specifies the manufacturer’s model number of the device.
    ///
    /// The recommended format is: "manufacturer,model", where manufacturer is a string describing the
    /// name of the manufacturer (such as a stock ticker symbol), and model specifies the model number.
    ///
    /// Example:
    ///
    ///     model = "fsl,MPC8349EMITX";
    Model(String),
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
    Status(StatusType),
    /// Describes the method by which a CPU in a disabled state is enabled. This property is required for CPUs with a status property with a value of "disabled". The value consists of one or more strings that define the method to release this CPU. If a client program recognizes any of the methods, it may use it.
    EnableMethod(EnableType),
    /// The cpu-release-addr property is required for cpu nodes that have an enable-method property value of "spin-table". The value specifies the physical address of a spin table entry that releases a secondary CPU from its spin loop.
    ReleaseAddr(U64),
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

impl Property {
    /// Attempts to parse a name and value into a property according to its predefined type.
    ///
    /// Returns `None` if the value coercion fails
    pub fn from_name_and_value(name: &str, value: &[u8]) -> Option<Self> {
        // match name {
        // "compatible" => value.try_into().ok().map(Self::Compatible),
        matcher!(
            name: value,
            "compatible" => Self::Compatible,
            "model" => Self::Model,
            "phandle" => Self::PHandle,
            "#address-cells" => Self::AddressCells,
            "#size-cells" => Self::SizeCells,
            "serial-number" => Self::SerialNumber,
            "bootargs" => Self::BootArgs,
            "device_type" => Self::DeviceType,
            "status" => Self::Status,
            "enable-method" => Self::EnableMethod,
            "cpu-release-addr" => Self::ReleaseAddr
        )
        // _ => Some(Self::Other(name.into(), value.into())),
        // }
    }
}

// parse a singular node
// fn parse_node(data: &[u8]) -> Option<Node<impl Allocator>> {}
