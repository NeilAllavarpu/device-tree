use core::ffi::CStr;

use alloc::rc::Rc;

use crate::parse::U32ByteSlice;

use super::{device, root, Node, PropertyKeys, PropertyMap, RawNode};

/// The `Chosen` node does not represent a real device in the system but describes parameters chosen or specified by the system firmware at run time.
#[derive(Debug)]
pub struct Chosen<'node> {
    /// A string that specifies the boot arguments for the client program.
    /// The value could potentially be a null string if no boot arguments are required.
    boot_args: Option<&'node CStr>,
    /// The node representing the device to be used for boot console output.
    stdout: Option<Rc<device::DeviceNode<'node>>>,
    /// The node representing the device to be used for boot console input.
    stdin: Option<Rc<device::DeviceNode<'node>>>,
    /// Any other properties under the `Chosen` node
    miscellaneous: PropertyMap<'node>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum ChosenError<'node> {
    BootArg(U32ByteSlice<'node>),
    StdoutPathInvalid(U32ByteSlice<'node>),
    StdoutDanglingPath(&'node CStr),
    StdinPathInvalid(U32ByteSlice<'node>),
    StdinDanglingPath(&'node CStr),
}

impl<'node> Chosen<'node> {
    /// Parses a raw node into the `/chosen` node
    pub(super) fn from_node<'root>(
        mut chosen: RawNode<'node>,
        root: &'root root::Node<'root>,
    ) -> Result<Self, ChosenError<'node>>
    where
        'root: 'node,
    {
        /// Extracts an `Rc` to the specified node from the given property
        fn rc_from_node<'root>(
            properties: &mut PropertyMap<'root>,
            property_key: &CStr,
            root: &'root root::Node<'root>,
        ) -> Result<Option<Rc<device::DeviceNode<'root>>>, ChosenError<'root>> {
            properties
                .remove(property_key)
                .map(|bytes| {
                    let c_string = <&CStr>::try_from(bytes)
                        .map_err(|_err| ChosenError::StdoutPathInvalid(bytes))?;
                    root.find_str(c_string.to_bytes())
                        .ok_or(ChosenError::StdoutDanglingPath(c_string))
                })
                .transpose()
        }

        if !chosen.children.is_empty() {
            unimplemented!("Children of the chosen node are currently not supported");
        }

        let boot_args = chosen
            .properties
            .remove(PropertyKeys::BOOTARGS)
            .map(|bytes| <&CStr>::try_from(bytes).map_err(|_err| ChosenError::BootArg(bytes)))
            .transpose()?;
        let stdout = rc_from_node(&mut chosen.properties, PropertyKeys::STDOUT_PATH, root)?;
        // If the stdin-path property is not specified, stdout-path should be assumed to define the input device.
        let stdin = rc_from_node(&mut chosen.properties, PropertyKeys::STDIN_PATH, root)?
            .or_else(|| stdout.as_ref().map(Rc::clone));

        Ok(Self {
            boot_args,
            stdout,
            stdin,
            miscellaneous: chosen.properties,
        })
    }

    #[must_use]
    #[inline]
    pub const fn boot_args(&self) -> Option<&CStr> {
        self.boot_args
    }

    #[must_use]
    #[inline]
    pub const fn stdout(&self) -> Option<&Rc<device::DeviceNode<'_>>> {
        self.stdout.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn stdin(&self) -> Option<&Rc<device::DeviceNode<'_>>> {
        self.stdin.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn properties(&self) -> &PropertyMap<'node> {
        &self.miscellaneous
    }
}
