//! Device tree blob parsing
//!
//! This crate parses a flattened device tree/device tree blob (DTB) from some location in memory, and uses minimal allocations to convert this into a convenient Rust format.

// #![no_std]
#![warn(clippy::all)]
#![warn(clippy::restriction)]
#![warn(clippy::complexity)]
#![deny(clippy::correctness)]
#![warn(clippy::nursery)]
#![warn(clippy::pedantic)]
#![deny(clippy::perf)]
#![warn(clippy::style)]
#![deny(clippy::suspicious)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::single_call_fn)]
#![allow(clippy::expect_used)]
#![allow(clippy::pub_use)]
#![allow(clippy::ref_patterns)]
#![allow(clippy::needless_borrowed_reference)]
#![allow(clippy::shadow_reuse)]
#![expect(clippy::blanket_clippy_restriction_lints, reason = "Paranoid linting")]
#![expect(clippy::implicit_return, reason = "Desired format")]
#![expect(clippy::question_mark_used, reason = "Desired format")]
#![feature(lint_reasons)]
#![feature(ascii_char)]
#![feature(iterator_try_collect)]
#![feature(array_chunks)]
#![feature(slice_split_once)]
#![feature(let_chains)]
#![feature(const_option)]
#![feature(const_result)]
#![feature(ptr_metadata)]
#![feature(ptr_from_ref)]
#![feature(stmt_expr_attributes)]
#![feature(iterator_try_reduce)]
#![feature(extract_if)]
#![feature(error_in_core)]
#![feature(const_ptr_as_ref)]
#![feature(pointer_is_aligned)]
#![feature(slice_ptr_get)]
#![feature(concat_bytes)]
#![feature(slice_take)]
#![feature(strict_provenance)]

extern crate alloc;

pub mod dtb;
mod map;
mod memory_reservation;
pub mod node;
mod node_name;
mod parse;
mod property;

/// Splits a slice at the first instance of the given value, returning the slice up to, but not including, said element, and the slice beginning immediately after.
/// In other words, returns the two slices formed by introducing a "hole" at the first matching element
///
/// Returns `None` if the value is not present in the slice
fn split_at_first<'slice, T: PartialEq>(
    slice: &'slice [T],
    value: &T,
) -> Option<(&'slice [T], &'slice [T])> {
    slice
        .iter()
        .enumerate()
        .find(|&(_, elem)| elem == value)
        .map(|(index, _)| {
            #[expect(clippy::indexing_slicing, reason = "This slicing should never panic since the index should always be less than the length of the slice")]
            (&slice[..index], &slice[index.checked_add(1).expect("This resulting should always be at most the length of the slice")..])
        })
}
