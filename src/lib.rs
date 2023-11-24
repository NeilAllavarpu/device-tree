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
#![feature(cstr_count_bytes)]
#![deny(unsafe_op_in_unsafe_fn)]
#![expect(clippy::single_call_fn, reason = "Desired code style")]
#![expect(
    clippy::expect_used,
    reason = "Only used when it should be unreachable"
)]
#![expect(clippy::ref_patterns, reason = "Desired code style")]
#![expect(clippy::needless_borrowed_reference, reason = "Desired code style")]
#![expect(clippy::shadow_reuse, reason = "Desired code style")]
#![expect(
    clippy::unreachable,
    reason = "Only used when it should be unreachable"
)]
#![expect(
    clippy::big_endian_bytes,
    reason = "Correctly used for big endian data"
)]
#![expect(clippy::missing_trait_methods, reason = "Desired code style")]
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
use core::{mem, num::NonZeroUsize, ptr::NonNull};

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

/// Transmutes a slice of one type into a slice of another type that is smaller than itself
///
/// # Safety
/// It must be valid to transmute a series of bytes, interpreted as `P`s, into a series of bytes, interpreted as `Q`s.
/// The sizes of individual elements do not have to match, but as slices the lengths computed must match
unsafe fn transmute_slice_down<P, Q>(slice: &[P]) -> &[Q] {
    let size_to =
        NonZeroUsize::new(mem::size_of::<Q>()).expect("Cannot transmute with zero-sized types");
    assert!(mem::size_of::<P>() % size_to == 0, "Can only transmute a slice down if the sizes of the types involved dont cross element boundaries");

    let transmuted_pointer = NonNull::slice_from_raw_parts(
        NonNull::from(slice).as_non_null_ptr().cast(),
        mem::size_of_val(slice) / size_to,
    );
    // SAFETY: The pointer is valid, accessible, and initalized because it comes from a valid, initialized region of memory;
    // the sizes align correctly as computed above;
    // and the caller promises that it is safe to transmute these types
    // The lifetime of the underlying data is guaranteed to be immutable because the original shared slice guarantees the underlying bytes are not mutated
    unsafe { transmuted_pointer.as_ref() }
}
