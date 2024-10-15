//! The core DTB parser and handler
//!
//! This module parses the device tree blob from memory and converts it into a convenient Rust object, on which you can call various methods to query the device tree

use crate::node::{cpu, RawNode};
use crate::node_name::NameRefError;
use crate::transmute_slice_down;
use crate::{map::Map, node::root, node_name::NameRef, parse::U32ByteSlice};
use alloc::boxed::Box;
use alloc::vec;
use alloc::{rc::Rc, vec::Vec};
use core::ffi::CStr;
use core::iter;
use core::mem;

/// The structure block is composed of a sequence of pieces, each beginning with a token, that is, a big-endian 32-bit integer.
/// Some tokens are followed by extra data, the format of which is determined by the token value.
/// All tokens shall be aligned on a 32-bit boundary,
/// which may require padding bytes (with a value of `0x0`) to be inserted after the previous token’s data.ß
enum Token<'token> {
    /// The `BeginNode` token marks the beginning of a node’s representation.
    /// It shall be followed by the node’s unit name as extra data.
    BeginNode(NameRef<'token>),
    /// The `EndNode` token marks the end of a node’s representation.
    /// This token has no extra data; so it is followed immediately by the next token, which may be any token except FDT_PROP.
    EndNode,
    /// The `Prop` token marks the beginning of the representation of one property in the devicetree.
    /// It shall be followed by extra data describing the property.
    /// This data consists first of the property’s length and name.
    /// After this structure, the property’s value is given as a byte string.
    /// This value is followed by zeroed padding bytes (if necessary) to align to the next 32-bit boundary and then the next token,
    /// which may be any token except `End`
    Prop(&'token CStr, U32ByteSlice<'token>),
    /// The `Nop` token will be ignored by any program parsing the device tree.
    /// This token has no extra data; so it is followed immediately by the next token, which can be any valid token.
    /// A property or node definition in the tree can be overwritten with `Nop` tokens to remove it from the tree without needing to move other sections of the tree’s representation in the devicetree blob.
    Nop,
    /// The `End` token marks the end of the structure block.
    /// There shall be only one `End` token, and it shall be the last token in the structure block.
    /// It has no extra data;
    /// so the byte immediately after the `End` token has offset from the beginning of the structure block equal to the value of the `size_dt_struct` field in the device tree blob header.
    End,
}

/// Errors that can occur while trying to consume a token
#[derive(Debug)]
#[non_exhaustive]
pub enum TokenError {
    /// The end of the byte stream was reached while parsing a token
    EoF,
    /// An invalid token type was encountered
    InvalidToken(u32),
    /// The node name in a `BeginNode` was malformed
    NodeNameMalformed,
    /// The node name in a `BeginNode` contained a character that was not permitted
    NodeNameInvalid(NameRefError),
    /// The value of a `Prop` was improperly defined
    PropValue,
    /// The name of a `Prop` was not a valid C string in the strings block
    PropName,
    /// The size of some offset was too large
    Size,
}

impl<'token> Token<'token> {
    /// The discriminant value for a `BeginNode` token
    const BEGIN_NODE: u32 = 0x1;
    /// The discriminant value for an `EndNode` token
    const END_NODE: u32 = 0x2;
    /// The discriminant value for a `Prop` token
    const PROP: u32 = 0x3;
    /// The discriminant value for a `Nop` token
    const NOP: u32 = 0x4;
    /// The discriminant value for an `End` token
    const END: u32 = 0x9;

    /// Parses a single token out of the byte stream, or fails with an error
    fn consume_token(
        bytes: &mut U32ByteSlice<'token>,
        strings: &'token [u8],
    ) -> Result<Self, TokenError> {
        match bytes.consume_u32().ok_or(TokenError::EoF)? {
            Self::BEGIN_NODE => {
                // The name is stored as a null-terminated string, and shall include the unit address (see Section 2.2.1), if any.
                // The node name is followed by zeroed padding bytes, if necessary for alignment, and then the next token, which may be any token except `FDT_END`.
                bytes
                    .consume_c_str()
                    .ok_or(TokenError::NodeNameMalformed)
                    .and_then(|name| {
                        NameRef::try_from(name.to_bytes()).map_err(TokenError::NodeNameInvalid)
                    })
                    .map(Self::BeginNode)
            }
            Self::END_NODE => Ok(Self::EndNode),
            Self::PROP => {
                // The length of the property’s value in bytes (which may be zero, indicating an empty property)
                let len = usize::try_from(bytes.consume_u32().ok_or(TokenError::EoF)?)
                    .map_err(|_err| TokenError::Size)?;

                // An offset into the strings block at which the property’s name is stored as a null-terminated string.
                let nameoff = usize::try_from(bytes.consume_u32().ok_or(TokenError::EoF)?)
                    .map_err(|_err| TokenError::Size)?;

                let value = bytes.take(len).ok_or(TokenError::PropValue)?;

                let name = strings
                    .get(nameoff..)
                    .and_then(|name| CStr::from_bytes_until_nul(name).ok())
                    .ok_or(TokenError::PropName)?;

                Ok(Self::Prop(name, value))
            }
            Self::NOP => Ok(Self::Nop),
            Self::END => Ok(Self::End),
            token => Err(TokenError::InvalidToken(token)),
        }
    }

    /// Creates an iterator over the provided byte stream that produces tokens one at a time, or fails if it encounters an invalid token
    fn iterate_bytes(
        mut bytes: U32ByteSlice<'token>,
        strings: &'token [u8],
    ) -> impl Iterator<Item = Result<Self, TokenError>> {
        iter::from_fn(move || (!bytes.is_empty()).then(|| Self::consume_token(&mut bytes, strings)))
    }
}

#[non_exhaustive]
#[derive(Debug)]
pub enum DeviceTreeError<'dtb> {
    /// The device tree, or some field inside the device tree, was not properly aligned
    Alignment,
    /// The magic bytes at the beginning of the device tree were incorrect
    Magic,
    /// The size or offset of some field is too large
    Size,
    /// An index (e.g. into the strings block) was invalid
    Index,
    /// An error occured while trying to parse a token and associated data
    Token(TokenError),
    /// The device tree ended before parsing could complete
    EoF,
    /// A string did not hold valid UTF8
    String,
    /// A property could not be decoded properly
    Property,
    /// A field held some sort of other invalid value not otherwise covered
    Parsing,
    /// An invalid token was encountered while parsing
    InvalidToken(u32),
    /// Error parsing nodes
    Node(root::NodeError<'dtb>),
    /// The boot CPU specified was invalid
    BootCpu(u32),
    TooManyEnds,
    InvalidProp,
    MismatchedNodes,
    BadRoots(Box<[NameRef<'dtb>]>),
    BadRootName(NameRef<'dtb>),
    BadDepth(usize),
    NewerVersion((u32, u32)),
    StringsIndex((usize, usize)),
    StructIndex((usize, usize)),
}

#[derive(Debug)]
pub struct DeviceTree<'dtb> {
    /// The root node of the device tree itself
    root: root::Node<'dtb>,
    /// This field shall contain the version of the devicetree data structure.
    /// The version is 17 if using the structure as parsed in this crate.
    /// An `DTSpec` boot program may provide the devicetree of a later version,
    /// in which case this field shall contain the version number defined in whichever later document gives the details of that version.
    ///
    /// Note: The version is with respect to the binary structure of the device tree, not its content.
    version: u32,
    /// This field shall contain the lowest version of the devicetree data structure with which the version used is backwards compatible.
    /// So, for the structure as parsed in this crate (version 17),
    /// this field shall contain 16 because version 17 is backwards compatible with version 16,
    /// but not earlier versions.
    /// A `DTSpec` boot program should provide a devicetree in a format which is backwards compatible with version 16,
    /// and thus this field shall always contain 16.
    last_compatible_version: u32,
    /// The system's boot CPU
    boot_cpu: Rc<cpu::Node<'dtb>>,
}

impl<'dtb> DeviceTree<'dtb> {
    /// The version of the DTB that we are parsing.
    /// The `last_compatible_version` should be no greater than this.
    pub const VERSION_PARSED: u32 = 17;
    /// Parses a device tree blob located at some point in memory.
    ///
    /// # Errors
    /// Returns an error if any part of the parsing process fails.
    /// See `DeviceTreeError` and associated errors for specific error conditions that are caught
    #[expect(clippy::unwrap_in_result, reason = "Checks should never fail")]
    #[expect(clippy::missing_panics_doc, reason = "Checks should never fail")]
    #[expect(clippy::too_many_lines)]
    #[inline]
    pub fn from_bytes(dtb: &'dtb [u64]) -> Result<Self, DeviceTreeError<'dtb>> {
        /// The magic bytes located at the start of the device tree
        const FDT_HEADER_MAGIC: u32 = 0xD00D_FEED;

        let binding = dtb.first().ok_or(DeviceTreeError::EoF)?.to_ne_bytes();
        let mut magic_and_size = binding.array_chunks::<{ mem::size_of::<u32>() }>();

        // This field shall contain the value 0xd00dfeed (big-endian).
        let fdt_header_magic = u32::from_be_bytes(
            *magic_and_size
                .next()
                .expect("Should be exactly two elements in the iterator"),
        );
        if fdt_header_magic != FDT_HEADER_MAGIC {
            return Err(DeviceTreeError::Magic);
        }

        // This field shall contain the total size in bytes of the devicetree data structure. This size shall encompass all sections of the structure: the header, the memory reservation block, structure block and strings block, as well as any free space gaps between the blocks or after the final block.
        let dt_size = usize::try_from(u32::from_be_bytes(
            *magic_and_size
                .next()
                .expect("Should be exactly two elements in the iterator"),
        ))
        .map_err(|_err| DeviceTreeError::Size)?;

        let dt_bytes = dtb
            .get(0..dt_size.div_ceil(mem::size_of::<u64>()))
            .ok_or(DeviceTreeError::EoF)?;
        // SAFETY: It is safe to transmute a `u64` to `u32`s
        let dt_bytes_u32: &[u32] = unsafe { transmute_slice_down(dt_bytes) };

        let mut dt_header =
            U32ByteSlice::new(dt_bytes_u32.get(0..10).ok_or(DeviceTreeError::EoF)?, 40)
                .expect("Length should be correct");

        dt_header.consume_u32(); // Magic, already checked
        dt_header.consume_u32(); // Size, already read

        // This field shall contain the offset in bytes of the structure block from the beginning of the header.
        let dt_struct_offset =
            usize::try_from(dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        // This field shall contain the offset in bytes of the strings block from the beginning of the header.
        let dt_strings_offset =
            usize::try_from(dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        // This field shall contain the offset in bytes of the memory reservation block from the beginning of the header.
        let mem_rsvmap_offset =
            usize::try_from(dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        let version = dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?;

        let last_compatible_version = dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?;

        if last_compatible_version > Self::VERSION_PARSED {
            return Err(DeviceTreeError::NewerVersion((
                version,
                last_compatible_version,
            )));
        }

        // This field shall contain the physical ID of the system’s boot CPU. It shall be identical to the physical ID given in the `reg` property of that CPU node within the devicetree.
        let boot_cpuid_phys = dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?;

        // This field shall contain the length in bytes of the strings block section of the devicetree blob.
        let dt_strings_size = usize::try_from(dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?)
            .map_err(|_err| DeviceTreeError::Size)?;
        // This field shall contain the length in bytes of the structure block section of the devicetree blob.
        let dt_struct_size = usize::try_from(dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?)
            .map_err(|_err| DeviceTreeError::Size)?;

        // Enforce alignment of the dt_struct to its proper size
        if dt_struct_offset % mem::size_of::<u32>() != 0
            || dt_struct_size % mem::size_of::<u32>() != 0
        {
            return Err(DeviceTreeError::Alignment);
        }
        let dt_struct_index = dt_struct_offset.div_ceil(4);
        let dt_struct_elems = dt_struct_size.div_ceil(4);

        let dt_struct = U32ByteSlice::new(
            dt_bytes_u32
                .get(
                    dt_struct_index
                        ..dt_struct_index
                            .checked_add(dt_struct_elems)
                            .ok_or(DeviceTreeError::Size)?,
                )
                .ok_or(DeviceTreeError::StructIndex((
                    dt_struct_offset,
                    dt_struct_size,
                )))?,
            dt_struct_size,
        )
        .expect("Length should be correct");

        // SAFETY: Transmuting a `u64` to multiple `u8`s is valid
        let dt_strings = unsafe { transmute_slice_down(dt_bytes) }
            .get(
                dt_strings_offset
                    ..dt_strings_offset
                        .checked_add(dt_strings_size)
                        .ok_or(DeviceTreeError::Size)?,
            )
            .ok_or(DeviceTreeError::StringsIndex((
                dt_strings_offset,
                dt_strings_size,
            )))?;

        let mut properties = Vec::new();
        let mut children = vec![Vec::new()];
        let mut names = Vec::new();
        let mut device_tree = Err(DeviceTreeError::EoF);
        for token in Token::iterate_bytes(dt_struct, dt_strings) {
            match token.map_err(DeviceTreeError::Token)? {
                Token::BeginNode(name) => {
                    properties.push(Map::new());
                    children.push(Vec::new());
                    names.push(name);
                }
                Token::EndNode => {
                    let name = names.pop().ok_or(DeviceTreeError::TooManyEnds)?;
                    let node = RawNode::new(
                        children
                            .pop()
                            .expect("Properties, children, and names should all be in sync"),
                        properties
                            .pop()
                            .expect("Properties, children, and names should all be in sync"),
                    );
                    children
                        .last_mut()
                        .ok_or(DeviceTreeError::TooManyEnds)?
                        .push((name, node));
                }
                Token::Prop(name, value) => {
                    if properties
                        .last_mut()
                        .ok_or(DeviceTreeError::InvalidProp)?
                        .insert(name, value)
                        .is_some()
                    {
                        // Duplicate property is bad
                        return Err(DeviceTreeError::Parsing);
                    }
                }
                Token::Nop => {}
                Token::End => {
                    if device_tree.is_ok() {
                        return Err(DeviceTreeError::TooManyEnds);
                    }

                    // The depth should be exactly one at the end, just the root node left to parse
                    if children.len() != 1 {
                        return Err(DeviceTreeError::BadDepth(children.len()));
                    }

                    let mut roots = children
                        .pop()
                        .expect("Depth of tree should be exactly one after the check");

                    // There should be exactly one root
                    if roots.len() != 1 {
                        return Err(DeviceTreeError::BadRoots(
                            roots.into_iter().map(|(name, _)| name).collect(),
                        ));
                    }

                    let (name, root) = roots
                        .pop()
                        .expect("Number of roots should be exactly one after the check");

                    // The full path to the root node is /
                    if !<&str>::from(name.node_name()).is_empty() || name.unit_address().is_some() {
                        return Err(DeviceTreeError::BadRootName(name));
                    }

                    let root: root::Node = root.try_into().map_err(DeviceTreeError::Node)?;

                    let boot_cpu = Rc::clone(
                        root.cpus()
                            .get(&boot_cpuid_phys)
                            .ok_or(DeviceTreeError::BootCpu(boot_cpuid_phys))?,
                    );
                    device_tree = Ok(Self {
                        root,
                        version,
                        last_compatible_version,
                        boot_cpu,
                    });
                }
            }
        }
        device_tree
    }

    #[inline]
    #[must_use]
    pub const fn root(&'dtb self) -> &root::Node<'dtb> {
        &self.root
    }

    #[must_use]
    #[inline]
    pub const fn boot_cpu(&self) -> &Rc<cpu::Node<'dtb>> {
        &self.boot_cpu
    }

    /// Returns the version of the device tree parsed
    #[must_use]
    #[inline]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the last compatible version of the device tree parsed; should be at least `VERSION_PARSED` for the parsing code in this crate
    #[must_use]
    #[inline]
    pub const fn last_compatible_version(&self) -> u32 {
        self.last_compatible_version
    }
}
