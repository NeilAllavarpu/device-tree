use core::ffi::CStr;

use alloc::rc::Rc;

use crate::parse::U32ByteSlice;

use super::{device, root, Node, PropertyKeys, PropertyMap, RawNode};

/// The `Chosen` node does not represent a real device in the system but describes parameters chosen or specified by the system firmware at run time.
#[derive(Debug)]
pub struct Chosen<'data> {
    /// A string that specifies the boot arguments for the client program.
    /// The value could potentially be a null string if no boot arguments are required.
    boot_args: Option<&'data CStr>,
    /// The node representing the device to be used for boot console output.
    stdout: Option<Rc<device::Node<'data>>>,
    /// The node representing the device to be used for boot console input.
    stdin: Option<Rc<device::Node<'data>>>,
    /// Any other properties under the `Chosen` node
    miscellaneous: PropertyMap<'data>,
    #[cfg(feature = "rpi")]
    /// The overlay_prefix string selected by config.txt.
    overlay_prefix: Option<&'data CStr>,
    #[cfg(feature = "rpi")]
    /// The os_prefix string selected by config.txt.
    os_prefix: Option<&'data CStr>,
    #[cfg(feature = "rpi")]
    /// The extended board revision code from OTP row 33.
    rpi_boardrev_ext: Option<u32>,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error<'data> {
    BootArg(U32ByteSlice<'data>),
    StdoutPathInvalid(U32ByteSlice<'data>),
    StdoutDanglingPath(&'data CStr),
    StdinPathInvalid(U32ByteSlice<'data>),
    StdinDanglingPath(&'data CStr),
    OverlayPrefix(U32ByteSlice<'data>),
    OsPrefix(U32ByteSlice<'data>),
    RpiBoardrevExt(U32ByteSlice<'data>),
}

impl<'data> Chosen<'data> {
    /// Parses a raw node into the `/chosen` node
    pub(super) fn from_node<'root>(
        mut chosen: RawNode<'data>,
        root: &'root root::Node<'data>,
    ) -> Result<Chosen<'data>, Error<'data>> {
        /// Extracts an `Rc` to the specified node from the given property
        fn rc_from_node<'data>(
            properties: &mut PropertyMap<'data>,
            property_key: &CStr,
            root: &root::Node<'data>,
        ) -> Result<Option<Rc<device::Node<'data>>>, Error<'data>> {
            properties
                .remove(property_key)
                .map(|bytes| {
                    let c_string =
                        <&CStr>::try_from(bytes).map_err(|_err| Error::StdoutPathInvalid(bytes))?;
                    root.find_str(c_string.to_bytes())
                        .ok_or(Error::StdoutDanglingPath(c_string))
                })
                .transpose()
        }

        if !chosen.children.is_empty() {
            unimplemented!("Children of the chosen node are currently not supported");
        }

        let boot_args = chosen
            .properties
            .remove(PropertyKeys::BOOTARGS)
            .map(|bytes| <&CStr>::try_from(bytes).map_err(|_err| Error::BootArg(bytes)))
            .transpose()?;
        let stdout = rc_from_node(&mut chosen.properties, PropertyKeys::STDOUT_PATH, root)?;
        // If the stdin-path property is not specified, stdout-path should be assumed to define the input device.
        let stdin = rc_from_node(&mut chosen.properties, PropertyKeys::STDIN_PATH, root)?
            .or_else(|| stdout.as_ref().map(Rc::clone));

        #[cfg(feature = "rpi")]
        let overlay_prefix = chosen
            .properties
            .remove(PropertyKeys::OVERLAY_PREFIX)
            .map(|bytes| <&CStr>::try_from(bytes).map_err(|_err| Error::OverlayPrefix(bytes)))
            .transpose()?;

        #[cfg(feature = "rpi")]
        let os_prefix = chosen
            .properties
            .remove(PropertyKeys::OS_PREFIX)
            .map(|bytes| <&CStr>::try_from(bytes).map_err(|_err| Error::OsPrefix(bytes)))
            .transpose()?;

        #[cfg(feature = "rpi")]
        let rpi_boardrev_ext = chosen
            .properties
            .remove(PropertyKeys::RPI_BOARDREV_EXT)
            .map(|bytes| u32::try_from(bytes).map_err(|_err| Error::RpiBoardrevExt(bytes)))
            .transpose()?;

        Ok(Self {
            boot_args,
            stdout,
            stdin,
            miscellaneous: chosen.properties,
            #[cfg(feature = "rpi")]
            overlay_prefix,
            #[cfg(feature = "rpi")]
            os_prefix,
            #[cfg(feature = "rpi")]
            rpi_boardrev_ext,
        })
    }

    #[must_use]
    #[inline]
    pub const fn boot_args(&self) -> Option<&CStr> {
        self.boot_args
    }

    #[must_use]
    #[inline]
    pub const fn stdout(&self) -> Option<&Rc<device::Node<'_>>> {
        self.stdout.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn stdin(&self) -> Option<&Rc<device::Node<'_>>> {
        self.stdin.as_ref()
    }

    #[must_use]
    #[inline]
    pub const fn properties(&self) -> &PropertyMap<'data> {
        &self.miscellaneous
    }
}

#[cfg(feature = "rpi")]
impl<'node> Chosen<'node> {
    #[must_use]
    #[inline]
    pub const fn overlay_prefix(&self) -> Option<&'node CStr> {
        self.overlay_prefix
    }

    #[must_use]
    #[inline]
    pub const fn os_prefix(&self) -> Option<&'node CStr> {
        self.os_prefix
    }

    #[must_use]
    #[inline]
    pub const fn rpi_boardrev_ext(&self) -> Option<u32> {
        self.rpi_boardrev_ext
    }
}
