use alloc::{borrow::ToOwned, vec::Vec};

use crate::{
    parse::{self, ByteParser},
    Node, Property,
};
use core::{mem, ptr::NonNull};
use std::println;

enum TokenType {
    BeginNode = 1,
    EndNode = 2,
    Prop = 3,
    Nop = 4,
    End = 9,
}

const TOKEN_BEGIN_NODE: u32 = 0x1;
const TOKEN_END_NODE: u32 = 0x2;
const TOKEN_PROP: u32 = 0x3;
const TOKEN_NOP: u32 = 0x4;
const TOKEN_END: u32 = 0x9;

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
}

#[derive(Debug)]
pub struct DeviceTree {
    root: Node,
    version: u32,
    last_compatible_version: u32,
    boot_cpuid: u32,
}

impl DeviceTree {
    /// Parses a device tree located at some point in memory. Catches and returns *some* errors, but not necessarily all
    /// # Safety
    /// The caller must ensure that there is a valid device tree blob located at the given pointer. In particular,
    /// * the entire memory range indicated by the header of the device tree must not be modified in any way during the parsing of the device tree
    #[expect(clippy::unwrap_in_result)]
    #[expect(clippy::panic_in_result_fn)]
    #[expect(clippy::too_many_lines)]
    pub unsafe fn from_raw(dt_addr: NonNull<u64>) -> Result<DeviceTree, DeviceTreeError> {
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
            // SAFETY: The caller promises that the device tree is safe to read
            unsafe { NonNull::slice_from_raw_parts(dt_addr.cast::<u8>(), dt_size).as_ref() };

        let mut dt_header = ByteParser::new(
            // SAFETY: See above
            unsafe { NonNull::slice_from_raw_parts(dt_addr.cast(), 10).as_ref() },
        );
        println!("{:X?}", dt_header);
        dt_header.consume_u32_be(); // Magic, already checked
        dt_header.consume_u32_be(); // Size, already read

        // This field shall contain the offset in bytes of the structure block from the beginning of the header.
        let dt_struct_offset =
            usize::try_from(dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        // This field shall contain the offset in bytes of the strings block from the beginning of the header.
        let dt_strings_offset =
            usize::try_from(dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        // This field shall contain the offset in bytes of the memory reservation block from the beginning of the header.
        let mem_rsvmap_offset =
            usize::try_from(dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        // This field shall contain the version of the devicetree data structure. The version is 17 if using the structure as defined in this document. An DTSpec boot program may provide the devicetree of a later version, in which case this field shall contain the version number defined in whichever later document gives the details of that version.
        let version = dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?;

        // This field shall contain the lowest version of the devicetree data structure with which the version used is backwards compatible. So, for the structure as defined in this document (version 17), this field shall contain 16 because version 17 is backwards compatible with version 16, but not earlier versions. A DTSpec boot program should provide a devicetree in a format which is backwards compatible with version 16, and thus this field shall always contain 16.
        let last_comp_version = dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?;

        // This field shall contain the physical ID of the system’s boot CPU. It shall be identical to the physical ID given in the `reg` property of that CPU node within the devicetree.
        let boot_cpuid_phys = dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?;

        // This field shall contain the length in bytes of the strings block section of the devicetree blob.
        let dt_strings_size =
            usize::try_from(dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;
        // This field shall contain the length in bytes of the structure block section of the devicetree blob.
        let dt_struct_size =
            usize::try_from(dt_header.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                .map_err(|_err| DeviceTreeError::Size)?;

        println!(
            "enter struct {:X} {:X} {:X} {:X} {:X}",
            dt_struct_offset,
            dt_struct_size,
            dt_strings_offset,
            dt_strings_size,
            dt_bytes.len()
        );
        let mut dt_struct = ByteParser::new(
            parse::u8_to_u32_slice(
                dt_bytes
                    .get(
                        dt_struct_offset
                            ..dt_struct_offset
                                .checked_add(dt_struct_size)
                                .ok_or(DeviceTreeError::Size)?,
                    )
                    .ok_or(DeviceTreeError::Index)?,
            )
            .ok_or(DeviceTreeError::Alignment)?,
        );

        let dt_strings = dt_bytes
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
        println!("enter loop");
        loop {
            let byte = dt_struct.consume_u32_be().ok_or(DeviceTreeError::EoF)?;
            match byte {
                TOKEN_BEGIN_NODE => {
                    let name = dt_struct
                        .consume_str()
                        .map_err(|_err| DeviceTreeError::String)?;

                    properties.push(Vec::new());
                    children.push(Vec::new());
                    names.push(name.into());
                }
                TOKEN_END_NODE => {
                    let node = Node::new(
                        names.pop().ok_or(DeviceTreeError::Token)?,
                        children.pop().ok_or(DeviceTreeError::Token)?,
                        properties.pop().ok_or(DeviceTreeError::Token)?,
                    );
                    children
                        .last_mut()
                        .ok_or(DeviceTreeError::Token)?
                        .push(node);
                }
                TOKEN_PROP => {
                    let len =
                        usize::try_from(dt_struct.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                            .map_err(|_err| DeviceTreeError::Size)?;

                    let nameoff =
                        usize::try_from(dt_struct.consume_u32_be().ok_or(DeviceTreeError::EoF)?)
                            .map_err(|_err| DeviceTreeError::Size)?;
                    let value = dt_struct.consume_bytes(len).ok_or(DeviceTreeError::EoF)?;

                    let name =
                        parse::parse_str(dt_strings.get(nameoff..).ok_or(DeviceTreeError::Index)?)
                            .map_err(|_err| DeviceTreeError::Parsing)?;

                    properties.last_mut().ok_or(DeviceTreeError::Token)?.push(
                        Property::from_name_and_value(name, value)
                            .ok_or(DeviceTreeError::Property)?,
                    );
                }
                TOKEN_NOP => {}
                TOKEN_END => {
                    let mut roots = children.pop().unwrap();
                    let root = roots.pop().ok_or(DeviceTreeError::Parsing)?;

                    if !roots.is_empty() {
                        return Err(DeviceTreeError::Parsing);
                    }

                    if !(names.is_empty() && properties.is_empty() && children.is_empty()) {
                        return Err(DeviceTreeError::EoF);
                    }

                    if !dt_struct.is_empty() {
                        return Err(DeviceTreeError::EoF);
                    }

                    return Ok(Self {
                        root,
                        version,
                        last_compatible_version: last_comp_version,
                        boot_cpuid: boot_cpuid_phys,
                    });
                }
                _ => {
                    return Err(DeviceTreeError::Parsing);
                }
            };
        }
    }

    pub fn get_node(&self, path: &str) -> Option<&Node> {
        let mut parent = &self.root;
        for chunk in path.split('/').filter(|x| !x.is_empty()) {
            parent = parent.get_child(chunk)?;
        }
        Some(parent)
    }
}
