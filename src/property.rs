//! Various properties that all device nodes may contain

use crate::{map::Map, node::PropertyKeys, parse::U32ByteSlice, split_at_first};
use alloc::string::String;
use alloc::{boxed::Box, vec::Vec};
use core::ffi::CStr;
use core::fmt;
use core::{ffi::FromBytesUntilNulError, fmt::Debug};

/// The model property value is a `<string>` that specifies the manufacturer’s model number of the device.
pub enum Model<'bytes> {
    /// The recommended format: "manufacturer,model", where manufacturer is a string describing the
    /// name of the manufacturer (such as a stock ticker symbol), and model specifies the model number.
    ///
    /// Example:
    ///
    ///     model = "fsl,MPC8349EMITX";
    ManufacturerModel(&'bytes [u8], &'bytes [u8]),
    /// An alternate format not of the manufacturer-model form
    Other(&'bytes [u8]),
}

impl fmt::Debug for Model<'_> {
    #[expect(clippy::min_ident_chars, reason = "Default parameter name")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::ManufacturerModel(arg0, arg1) => f
                .debug_tuple("ManufacturerModel")
                .field(&String::from_utf8_lossy(arg0))
                .field(&String::from_utf8_lossy(arg1))
                .finish(),
            Self::Other(arg0) => f
                .debug_tuple("Other")
                .field(&String::from_utf8_lossy(arg0))
                .finish(),
        }
    }
}

impl<'bytes> From<&'bytes CStr> for Model<'bytes> {
    fn from(value: &'bytes CStr) -> Self {
        let value = value.to_bytes();
        split_at_first(value, &b',').map_or(Self::Other(value), |(manufacturer, model)| {
            Self::ManufacturerModel(manufacturer, model)
        })
    }
}

impl<'bytes> TryFrom<U32ByteSlice<'bytes>> for Box<[Model<'bytes>]> {
    type Error = FromBytesUntilNulError;

    #[inline]
    #[expect(clippy::unwrap_in_result, reason = "Checks should never fail")]
    fn try_from(value: U32ByteSlice<'bytes>) -> Result<Self, Self::Error> {
        let mut value = <&[u8]>::from(value);
        let mut models = Vec::new();

        while !value.is_empty() {
            let model = Model::from(CStr::from_bytes_until_nul(value)?);
            let length = match model {
                Model::ManufacturerModel(manufacturer, model) => manufacturer
                    .len()
                    .checked_add(2) // 1 for nul byte, 1 for comma
                    .and_then(|length| length.checked_add(model.len()))
                    .expect("The nul byte should have already been found"),
                Model::Other(&[]) => {
                    value
                        .take(..1) // Adjust for the nul byte
                        .expect("CStr should not go past the end of the slice");
                    continue;
                }
                Model::Other(string) => string
                    .len()
                    .checked_add(1)
                    .expect("The nul byte should have already been found"),
            };

            value
                .take(..length)
                .expect("CStr should not go past the end of the slice");

            models.push(model);
        }

        Ok(models.into_boxed_slice())
    }
}

/// The `ranges` property provides a means of defining a mapping or translation between the address space of the bus (the child address space) and the address space of the bus node’s parent (the parent address space).
#[derive(Debug)]
pub struct Range {
    /// The `child-bus-address` is a physical address within the child bus’ address space.
    pub child_address: u64,
    /// `The parent-bus-address` is a physical address within the parent bus’ address space.
    pub parent_address: u64,
    /// The `length` specifies the size of the range in the child’s address space.
    pub length: u64,
}

impl From<[u64; 3]> for Range {
    fn from([child_address, parent_address, size]: [u64; 3]) -> Self {
        Self {
            child_address,
            parent_address,
            length: size,
        }
    }
}

/// The `status` property indicates the operational status of a device.
/// The lack of a `status` property should be treated as if the property existed with the value of `Ok`.
#[derive(Debug)]
pub enum Status<'bytes> {
    /// Indicates the device is operational.
    Ok,
    /// Indicates that the device is not presently operational, but it might become operational in the future
    /// (for example, something is not plugged in, or switched off).
    ///
    /// Refer to the device binding for details on what disabled means for a given device.
    Disabled,
    /// Indicates that the device is operational, but should not be used.
    /// Typically this is used for devices that are controlled by another software component, such as platform firmware.
    Reserved,
    /// Indicates that the device is not operational.
    /// A serious error was detected in the device, and it is unlikely to become operational without repair.
    ///
    /// The byte portion of the value is specific to the device and indicates the error condition detected.
    Fail(Option<&'bytes [u8]>),
}

/// Errors from converting a property value into a status
pub enum StatusError<'bytes> {
    /// The status was not a valid null-terminated string
    NotCStr(<&'bytes CStr as TryFrom<U32ByteSlice<'bytes>>>::Error),
    /// The value of the status was not one of the defined values
    InvalidValue,
}

impl<'bytes> TryFrom<U32ByteSlice<'bytes>> for Status<'bytes> {
    type Error = StatusError<'bytes>;

    fn try_from(value: U32ByteSlice<'bytes>) -> Result<Self, Self::Error> {
        <&CStr>::try_from(value)
            .map_err(StatusError::NotCStr)
            .and_then(|string| {
                let string = string.to_bytes();
                string.strip_prefix(b"fail").map_or_else(
                    || match string {
                        b"okay" => Ok(Self::Ok),
                        b"disabled" => Ok(Self::Disabled),
                        b"reserved" => Ok(Self::Reserved),
                        _ => Err(StatusError::InvalidValue),
                    },
                    |mut code| {
                        code.take_first().map_or(Ok(Self::Fail(None)), |&x| {
                            if x == b'-' {
                                Ok(Self::Fail(Some(code)))
                            } else {
                                Err(StatusError::InvalidValue)
                            }
                        })
                    },
                )
            })
    }
}

/// Describes the method by which a CPU in a disabled state is enabled.
/// This property is required for CPUs with a status property with a value of `Disabled`.
/// The value consists of one or more strings that define the method to release this CPU.
/// If a client program recognizes any of the methods, it may use it.
#[derive(Debug)]
pub enum EnableMethod<'bytes> {
    /// The CPU is enabled with the spin table method defined in the `DTSpec`.
    SpinTable(u64),
    /// Implementation dependent string that describes the method by which a CPU is released from a "disabled" state.
    ///
    /// The required format is: `"[vendor],[method]"`,
    /// where `vendor` is a string describing the name of the manufacturer and `method` is a string describing the vendor specific mechanism.
    ///
    /// Example: `"fsl,MPC8572DS"`
    VendorSpecific(&'bytes [u8], &'bytes [u8]),
}

/// Errors from parsing an enable method property
pub enum EnableMethodError {
    /// The property is not present
    NotPresent,
    /// The property specified `SpinTable` but no release address was found
    NoReleaseAddr,
    /// The property is in another invalid format
    Invalid,
}

impl<'prop> EnableMethod<'prop> {
    /// Extracts and parses `EnableType` from a map of properties, returning the type if valid
    pub(crate) fn extract_from_properties(
        properties: &mut Map<&'prop CStr, U32ByteSlice<'prop>>,
    ) -> Result<Self, EnableMethodError> {
        properties
            .remove(PropertyKeys::ENABLE_METHOD)
            .ok_or(EnableMethodError::NotPresent)
            .and_then(|bytes| <&CStr>::try_from(bytes).map_err(|_err| EnableMethodError::Invalid))
            .and_then(|method| match method.to_bytes() {
                b"spin-table" => Ok(EnableMethod::SpinTable({
                    properties
                        .remove(PropertyKeys::CPU_RELEASE_ADDR)
                        .and_then(|addr| u64::try_from(addr).ok())
                        .ok_or(EnableMethodError::NoReleaseAddr)?
                })),
                string => {
                    let mut chunks = string.split(|&character| character == b',');
                    let vendor = chunks.next().ok_or(EnableMethodError::Invalid)?;
                    let vendor_method = chunks.next().ok_or(EnableMethodError::Invalid)?;
                    if chunks.next().is_some() {
                        Err(EnableMethodError::Invalid)
                    } else {
                        Ok(Self::VendorSpecific(vendor, vendor_method))
                    }
                }
            })
    }
}

/// Specifies a string that identifies the form-factor of the system.
#[derive(Debug)]
#[expect(clippy::missing_docs_in_private_items, reason = "Self explanatory")]
pub enum ChassisType {
    Desktop,
    Laptop,
    Convertible,
    Server,
    Tablet,
    Handset,
    Watch,
    Embedded,
}

/// Errors from parsing the chassis field on a root node
#[derive(Debug)]
pub enum ChassisError<'bytes> {
    /// Error converting the bytes into a C string
    CStr(<&'bytes CStr as TryFrom<U32ByteSlice<'bytes>>>::Error),
    /// Value of the field did not match one of the defined values
    Invalid,
}

impl<'bytes> TryFrom<U32ByteSlice<'bytes>> for ChassisType {
    type Error = ChassisError<'bytes>;

    fn try_from(value: U32ByteSlice<'_>) -> Result<Self, Self::Error> {
        <&CStr>::try_from(value)
            .map_err(ChassisError::CStr)
            .and_then(|string| match string.to_bytes() {
                b"desktop" => Ok(Self::Desktop),
                b"laptop" => Ok(Self::Laptop),
                b"convertible" => Ok(Self::Convertible),
                b"server" => Ok(Self::Server),
                b"tablet" => Ok(Self::Tablet),
                b"handset" => Ok(Self::Handset),
                b"watch" => Ok(Self::Watch),
                b"embedded" => Ok(Self::Embedded),
                _ => Err(ChassisError::Invalid),
            })
    }
}
