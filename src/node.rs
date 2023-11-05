use crate::Property;
use alloc::boxed::Box;

/// A Device Tree Node
#[derive(Debug)]
pub struct Node {
    name: Box<str>,
    children: Box<[Node]>,
    properties: Box<[Property]>,
}

impl Node {
    /// Returns the child corresponding to the given name, if present
    pub(crate) fn get_child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|x| (x.name).as_ref() == name)
    }

    /// Creates a node with the given name, children, and properties
    pub(crate) fn new(
        name: Box<str>,
        children: impl IntoIterator<Item = Node>,
        mut properties: impl IntoIterator<Item = Property>,
    ) -> Self {
        // if let Some((spin_index, _)) = properties
        //     .iter()
        //     .enumerate()
        //     .find(|(_, x)| matches!(x, Property::EnableMethod(EnableType::SpinTable(_))))
        // {
        //     let (index, _) = properties
        //         .iter()
        //         .enumerate()
        //         .find(|(_, x)| matches!(x, Property::ReleaseAddr(_)))?;

        //     let Property::ReleaseAddr(U64(addr)) = properties[index] else {
        //         unreachable!()
        //     };
        //     properties[spin_index] = Property::EnableMethod(EnableType::SpinTable(addr));
        //     properties.swap_remove(index);
        // }
        Self {
            name,
            children: children.into_iter().collect(),
            properties: properties.into_iter().collect(),
        }
    }
}
