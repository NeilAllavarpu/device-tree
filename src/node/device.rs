use core::ffi::CStr;

use alloc::{
    boxed::Box,
    rc::{Rc, Weak},
};

use crate::{
    map::Map,
    property::{Model, Range, Status},
};

use super::{
    interrupt::PartialInterruptDevice, ChildMap, PropertyKeys, PropertyMap, RawNode, RawNodeError,
};

/// A Device Tree Node
#[derive(Debug)]
pub struct Node<'data> {
    /// Children of this node
    children: ChildMap<'data>,
    /// The compatible property value consists of one or more strings that define the specific programming model for the device.
    /// This list of strings should be used by a client program for device driver selection.
    /// The property value consists of a concatenated list of null terminated strings, from most specific to most general.
    /// They allow a device to express its compatibility with a family of similar devices, potentially allowing a single device driver to match against several devices.
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
    compatible: Option<Box<[Model<'data>]>>,
    ///  The model property value is a `<string>` that specifies the manufacturer’s model number of the device.
    model: Option<Model<'data>>,
    /// The `reg` property describes the address of the device’s resources within the address space defined by its parent bus.
    /// Most commonly this means the offsets and lengths of memory-mapped IO register blocks, but may have a different meaning on some bus types.
    /// Addresses in the address space defined by the root node are CPU real addresses.
    reg: Option<Box<[[u64; 2]]>>,
    /// The `ranges`` property provides a means of defining a mapping or translation between the address space of the bus (the child address space) and the address space of the bus node’s parent (the parent address space).
    ranges: Option<Box<[Range]>>,
    /// The status property indicates the operational status of a device.
    status: Status<'data>,
    /// Miscellaneous extra properties regarding this node
    pub(super) properties: PropertyMap<'data>,
    /// Interrupt information about the device
    interrupts: Rc<PartialInterruptDevice<'data>>,
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
    BadPHandle,
    DuplicatePHandle,
    Child(Box<Error>),
}

impl<'node> Node<'node> {
    /// Constructs a new `DeviceNode` from a given `RawNode` and additional properties
    pub(super) fn new<'phandles>(
        mut value: RawNode<'node>,
        address_cells: Option<u8>,
        size_cells: Option<u8>,
        phandles: &'phandles mut Map<u32, Rc<Node<'node>>>,
        parent: Option<&Weak<Node<'node>>>,
    ) -> Result<Rc<Node<'node>>, Error> {
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
            .map(|bytes| bytes.try_into().map_err(|_err| Error::Compatible))
            .transpose()?;
        let model = value
            .properties
            .remove(&PropertyKeys::MODEL)
            .map(|bytes| {
                <&CStr>::try_from(bytes)
                    .map(Model::from)
                    .map_err(|_err| Error::Model)
            })
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
                Status::try_from(bytes).map_err(|_err| Error::Status)
            })?;

        let phandle = value
            .properties
            .remove(PropertyKeys::PHANDLE)
            .map(u32::try_from)
            .transpose()
            .map_err(|_err| Error::BadPHandle)?;

        let node = Rc::new_cyclic(|device| {
            let (mut properties, children) = value.into_components_from_cells(
                child_address_cells.ok(),
                child_size_cells.ok(),
                phandles,
                Some(device),
            );

            Self {
                children: children
                    .map_err(|err| match err {
                        RawNodeError::Cells => Error::Cells,
                        RawNodeError::Child(child) => Error::Child(Box::new(child)),
                    })
                    .unwrap(),
                compatible,
                model,
                reg,
                ranges,
                status,
                interrupts: Rc::new(PartialInterruptDevice::extract_from_properties(
                    &mut properties,
                    Weak::clone(device),
                    parent,
                )),
                properties,
            }
        });

        if let Some(phandle) = phandle {
            if phandles.insert(phandle, Rc::clone(&node)).is_some() {
                return Err(Error::DuplicatePHandle);
            };
        }
        Ok(node)
    }

    #[must_use]
    #[inline]
    pub fn compatible(&self) -> Option<&[Model<'_>]> {
        self.compatible.as_deref()
    }

    #[must_use]
    #[inline]
    pub const fn model(&self) -> Option<&Model<'_>> {
        self.model.as_ref()
    }

    #[must_use]
    #[inline]
    pub fn reg(&self) -> Option<&[[u64; 2]]> {
        self.reg.as_deref()
    }

    #[must_use]
    #[inline]
    pub fn ranges(&self) -> Option<&[Range]> {
        self.ranges.as_deref()
    }

    #[must_use]
    #[inline]
    pub const fn status(&self) -> &Status<'node> {
        &self.status
    }

    #[must_use]
    #[inline]
    pub fn interrupts(&self) -> &PartialInterruptDevice<'_> {
        &self.interrupts
    }
}

impl<'node> super::Node<'node> for Node<'node> {
    #[inline]
    fn properties(&self) -> &PropertyMap {
        &self.properties
    }

    #[inline]
    fn children(&self) -> &ChildMap<'node> {
        &self.children
    }
}
