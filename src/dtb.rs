//! The core DTB parser and handler
//!
//! This module parses the device tree blob from memory and converts it into a convenient Rust object, on which you can call various methods to query the device tree

use crate::node::cpu;
use crate::{
    map::Map, node::root, node::RawNode, node_name::NameRef, parse::U32ByteSlice,
    property::to_c_str,
};
use alloc::{rc::Rc, vec::Vec};
use core::{
    ffi::CStr,
    fmt::{Debug, Display},
};
use core::{mem, ptr::NonNull};

use std::println;

const TOKEN_BEGIN_NODE: u32 = 0x1;
const TOKEN_END_NODE: u32 = 0x2;
const TOKEN_PROP: u32 = 0x3;
const TOKEN_NOP: u32 = 0x4;
const TOKEN_END: u32 = 0x9;

#[non_exhaustive]
#[derive(Debug)]
pub enum DeviceTreeError {
    /// The device tree, or some field inside the device tree, was not properly aligned
    Alignment,
    /// The magic bytes at the beginning of the device tree were incorrect
    Magic,
    /// The size or offset of some field is too large
    Size,
    /// An index (e.g. into the strings block) was invalid
    Index,
    /// An invalid token was received, or a token was received in the wrong position
    Token,
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
}

impl Display for DeviceTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as Debug>::fmt(self, f)
    }
}

impl core::error::Error for DeviceTreeError {}

#[derive(Debug)]
pub struct DeviceTree<'a> {
    root: root::Node<'a>,
    version: u32,
    // aliases: HashMap<Name, Box<str>>,
    boot_args: Option<Box<str>>,
    stdout_path: Option<Box<str>>,
    stdin_path: Option<Box<str>>,
    last_compatible_version: u32,
    boot_cpu: Rc<cpu::Node<'a>>,
}

impl<'a> DeviceTree<'a> {
    fn aliases() -> NameRef<'static> {
        b"aliases".as_slice().try_into().unwrap()
    }
    fn chosen() -> NameRef<'static> {
        b"chosen".as_slice().try_into().unwrap()
    }

    // const CHOSEN: NodeNameRef<'static> = b"chosen".as_slice().try_into().unwrap();
    const BOOTARGS: &'static CStr = to_c_str(b"bootargs\0");
    const STDIN_PATH: &'static CStr = to_c_str(b"stdin-path\0");
    const STDOUT_PATH: &'static CStr = to_c_str(b"stdout-path\0");
    /// Parses a device tree located at some point in memory. Catches and returns *some* errors, but not necessarily all
    /// # Safety
    /// The caller must ensure that there is a valid device tree blob located at the given pointer,
    /// *plus padding to treat the memory as a sequence of `u32`s*. In particular,
    /// * the entire memory range indicated by the header of the device tree must not be modified in
    /// any way during the parsing of the device tree, *including the padding*
    #[expect(clippy::unwrap_in_result)]
    #[expect(clippy::panic_in_result_fn)]
    #[expect(clippy::too_many_lines)]
    #[inline]
    pub unsafe fn from_raw(dt_addr: NonNull<u64>) -> Result<Self, DeviceTreeError> {
        /// The magic bytes located at the start of the device tree
        const FDT_HEADER_MAGIC: u32 = 0xD00D_FEED;

        if !dt_addr.as_ptr().is_aligned() {
            return Err(DeviceTreeError::Alignment);
        }

        // This field shall contain the value 0xd00dfeed (big-endian).
        let fdt_header_magic = u32::from_be(
            // SAFETY: The caller promises that the device tree is safe to read
            unsafe { dt_addr.cast::<u32>().as_ptr().read() },
        );
        if fdt_header_magic != FDT_HEADER_MAGIC {
            return Err(DeviceTreeError::Magic);
        }

        let dt_size_addr = dt_addr
            .addr()
            .checked_add(mem::size_of::<u32>())
            .ok_or(DeviceTreeError::EoF)?;

        // This field shall contain the total size in bytes of the devicetree data structure. This size shall encompass all sections of the structure: the header, the memory reservation block, structure block and strings block, as well as any free space gaps between the blocks or after the final block.
        let dt_size = usize::try_from(u32::from_be(
            // SAFETY: The caller promises that the device tree is safe to read
            unsafe {
                dt_addr
                    .with_addr(dt_size_addr)
                    .cast::<u32>()
                    .as_ptr()
                    .read()
            },
        ))
        .map_err(|_err| DeviceTreeError::Size)?;

        let dt_bytes =
            // SAFETY: The caller promises that the device tree is safe to read, and the padding is guaranteed as well
            unsafe { NonNull::slice_from_raw_parts(dt_addr.cast::<u32>(), dt_size.div_ceil(mem::size_of::<u32>())).as_ref() };

        let mut dt_header = U32ByteSlice::new(dt_bytes.get(0..10).ok_or(DeviceTreeError::EoF)?);

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

        // This field shall contain the version of the devicetree data structure. The version is 17 if using the structure as defined in this document. An DTSpec boot program may provide the devicetree of a later version, in which case this field shall contain the version number defined in whichever later document gives the details of that version.
        let version = dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?;

        // This field shall contain the lowest version of the devicetree data structure with which the version used is backwards compatible. So, for the structure as defined in this document (version 17), this field shall contain 16 because version 17 is backwards compatible with version 16, but not earlier versions. A DTSpec boot program should provide a devicetree in a format which is backwards compatible with version 16, and thus this field shall always contain 16.
        let last_comp_version = dt_header.consume_u32().ok_or(DeviceTreeError::EoF)?;

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

        let mut dt_struct = U32ByteSlice::new(
            dt_bytes
                .get(
                    dt_struct_index
                        ..dt_struct_index
                            .checked_add(dt_struct_elems)
                            .ok_or(DeviceTreeError::Size)?,
                )
                .ok_or(DeviceTreeError::Index)?,
        );

        let dt_strings = unsafe {
            NonNull::slice_from_raw_parts(NonNull::from(dt_bytes).as_non_null_ptr().cast(), dt_size)
                .as_ref()
        }
        .get(
            dt_strings_offset
                ..dt_strings_offset
                    .checked_add(dt_strings_size)
                    .ok_or(DeviceTreeError::Size)?,
        )
        .ok_or(DeviceTreeError::Index)?;

        let mut properties = Vec::new();
        let mut children = vec![Vec::new()];
        let mut names = Vec::new();

        loop {
            let byte = dt_struct.consume_u32().ok_or(DeviceTreeError::EoF)?;
            match byte {
                TOKEN_BEGIN_NODE => {
                    let name = dt_struct.consume_c_str().unwrap();

                    properties.push(Map::new());
                    children.push(Vec::new());
                    names.push(name.to_bytes().try_into().unwrap());
                }
                TOKEN_END_NODE => {
                    let name = names.pop().ok_or(DeviceTreeError::Token)?;
                    let node = RawNode::new(
                        children.pop().ok_or(DeviceTreeError::Token)?,
                        properties.pop().ok_or(DeviceTreeError::Token)?,
                    );
                    children
                        .last_mut()
                        .ok_or(DeviceTreeError::Token)?
                        .push((name, node));
                }
                TOKEN_PROP => {
                    let len = usize::try_from(dt_struct.consume_u32().ok_or(DeviceTreeError::EoF)?)
                        .map_err(|_err| DeviceTreeError::Size)?;

                    let nameoff =
                        usize::try_from(dt_struct.consume_u32().ok_or(DeviceTreeError::EoF)?)
                            .map_err(|_err| DeviceTreeError::Size)?;
                    let value = dt_struct.take((len + 3) / 4).ok_or(DeviceTreeError::EoF)?;

                    let name = CStr::from_bytes_until_nul(
                        dt_strings.get(nameoff..).ok_or(DeviceTreeError::Index)?,
                    )
                    .map_err(|_err| DeviceTreeError::Parsing)
                    .unwrap();

                    if properties
                        .last_mut()
                        .ok_or(DeviceTreeError::Token)?
                        .insert(name, value)
                        .is_some()
                    {
                        // Duplicate property is bad
                        panic!("h");
                        return Err(DeviceTreeError::Parsing);
                    }
                }
                TOKEN_NOP => {}
                TOKEN_END => {
                    let mut roots = children.pop().unwrap();
                    let (name, mut root) = roots.pop().ok_or(DeviceTreeError::Parsing).unwrap();

                    if !roots.is_empty() {
                        unreachable!();
                        return Err(DeviceTreeError::Parsing);
                    }

                    assert_eq!(name, b"".as_slice().try_into().unwrap());

                    if !(names.is_empty() && properties.is_empty() && children.is_empty()) {
                        return Err(DeviceTreeError::EoF);
                    }

                    if !dt_struct.is_empty() {
                        return Err(DeviceTreeError::EoF);
                    }

                    let aliases_node = root.children.remove(&Self::aliases()).unwrap();
                    // let aliases = aliases_node
                    //     .properties
                    //     .into_iter()
                    //     .map(|(name, value)| {
                    //         (
                    //             name.into(),
                    //             CStr::from_bytes_until_nul(value.into())
                    //                 .unwrap()
                    //                 .to_str()
                    //                 .unwrap()
                    //                 .into(),
                    //         )
                    //     })
                    //     .collect();
                    assert!(aliases_node.children.is_empty());

                    let mut chosen_node = root
                        .children
                        .remove(&Self::chosen())
                        .ok_or(DeviceTreeError::Parsing)
                        .unwrap();
                    assert!(chosen_node.children.is_empty());

                    let boot_args = chosen_node
                        .properties
                        .remove(&Self::BOOTARGS)
                        .map(|x| <&CStr>::try_from(x).unwrap().to_str().unwrap().into());
                    let stdin_path = chosen_node
                        .properties
                        .remove(&Self::STDIN_PATH)
                        .map(|x| <&CStr>::try_from(x).unwrap().to_str().unwrap().into());
                    let stdout_path = chosen_node
                        .properties
                        .remove(&Self::STDOUT_PATH)
                        .map(|x| <&CStr>::try_from(x).unwrap().to_str().unwrap().into());

                    if !chosen_node.properties.is_empty() {
                        println!(
                            "WARNING: ignoring properties for chosen {:?}",
                            chosen_node.properties
                        );
                    }

                    let root: root::Node = root.try_into().unwrap();
                    // .map_err(|_err| DeviceTreeError::Parsing)
                    // .unwrap();
                    // println!(
                    //     "hi {:?}",
                    //     root.node
                    //         .children
                    //         .get("soc")
                    //         .unwrap()
                    //         .children
                    //         .get("serial@7e201000")
                    //         .unwrap()
                    //         .properties
                    // );

                    // println!(
                    //     "{:?}",
                    //     root.node
                    //         .children
                    //         .get(&("soc".into(), None))
                    //         .unwrap()
                    //         .children
                    //         .get(&("serial".into(), Some(0x7E20_1000)))
                    //         .unwrap()
                    //         .children
                    //         .get(&("bluetooth".into(), None))
                    //         .unwrap()
                    // );
                    let boot_cpu = root.cpus.get(&boot_cpuid_phys).unwrap().clone();

                    return Ok(Self {
                        root,
                        // aliases,
                        version,
                        last_compatible_version: last_comp_version,
                        boot_cpu,
                        boot_args,
                        stdout_path,
                        stdin_path,
                    });
                }
                token => {
                    return Err(DeviceTreeError::InvalidToken(token));
                }
            };
        }
    }

    #[inline]
    pub fn get_root(&self) -> &root::Node {
        &self.root
    }

    pub fn cpus(&self) -> &Map<u32, Rc<cpu::Node>> {
        &self.root.cpus
    }

    pub fn boot_cpu(&self) -> Rc<cpu::Node> {
        self.boot_cpu.clone()
    }
}
