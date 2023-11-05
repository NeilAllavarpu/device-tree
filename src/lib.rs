// #![no_std]
#![feature(lint_reasons)]
#![feature(pointer_is_aligned)]
#![feature(slice_ptr_get)]
#![feature(slice_take)]
#![feature(strict_provenance)]

extern crate alloc;

mod device_tree;
mod node;
mod parse;
mod property;

pub use device_tree::{DeviceTree, DeviceTreeError};
pub use node::Node;
pub use property::Property;
