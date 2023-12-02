use super::{
    device::{self, Node},
    PropertyKeys, PropertyMap,
};
use crate::parse::U32ByteSlice;
use alloc::rc::Weak;

#[derive(Debug)]
/// The two representations for a parent of an interrupt node
enum Parent<'node> {
    /// A direct phandle to some other node is provided
    PHandle(u32),
    /// The direct parent is implicitly the device-tree parent
    DirectParent(Weak<device::Node<'node>>),
}

#[derive(Debug)]
pub struct PartialInterruptDevice<'node> {
    /// The device that this interrupt device belongs to
    device: Weak<device::Node<'node>>,
    /// The interrupt parent of this device
    interrupt_parent: Option<Parent<'node>>,
    /// Whether or not this is an interrupt controller
    is_controller: bool,
    /// Interrupt cell count
    cells: Option<u8>,
    /// The interrupts property of this node
    interrupts: Option<U32ByteSlice<'node>>,
    /// The interrupts map property of this node
    interrupt_map: Option<U32ByteSlice<'node>>,
    /// The interrupts mask property of this node
    interrupt_map_mask: Option<U32ByteSlice<'node>>,
}

impl<'node> PartialInterruptDevice<'node> {
    /// Extracts a partial interrupt device from the properties of a node.
    pub(super) fn extract_from_properties(
        properties: &mut PropertyMap<'node>,
        device: Weak<Node<'node>>,
        device_parent: Option<&Weak<Node<'node>>>,
        // device_parent
    ) -> Self {
        let is_controller = properties
            .remove(PropertyKeys::INTERRUPT_CONTROLLER)
            .is_some();
        let cells = properties
            .remove(PropertyKeys::INTERRUPT_CELLS)
            .map(|bytes| u32::try_from(bytes).unwrap().try_into().unwrap());
        let interrupts = properties.remove(PropertyKeys::INTERRUPTS);
        let interrupt_parent = properties
            .remove(PropertyKeys::INTERRUPT_PARENT)
            .map(|x| Parent::PHandle(u32::try_from(x).unwrap()))
            .or_else(|| device_parent.map(|x| Parent::DirectParent(Weak::clone(x))));
        let interrupt_map = properties.remove(PropertyKeys::INTERRUPT_MAP);
        let interrupt_map_mask = properties.remove(PropertyKeys::INTERRUPT_MAP_MASK);
        Self {
            device,
            interrupt_parent,
            is_controller,
            cells,
            interrupts,
            interrupt_map,
            interrupt_map_mask,
        }
    }
}

// #[derive(Debug)]
// pub struct Generator<'node> {
//     device: Weak<device::DeviceNode<'node>>,
//     /// Because the hierarchy of the nodes in the interrupt tree might not match the devicetree, the interrupt-parent property is available to make the definition of an interrupt parent explicit. The value is the phandle to the interrupt parent. If this property is missing from a device, its interrupt parent is assumed to be its devicetree parent.
//     parent: Option<Weak<InterruptDevice<'node>>>,
//     interrupts: Box<()>,
// }
