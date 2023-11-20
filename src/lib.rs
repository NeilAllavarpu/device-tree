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
#![feature(extract_if)]
#![feature(error_in_core)]
#![feature(const_ptr_as_ref)]
#![feature(pointer_is_aligned)]
#![feature(slice_ptr_get)]
#![feature(slice_take)]
#![feature(strict_provenance)]

extern crate alloc;

pub mod dtb;
mod map;
mod memory_reservation;
mod node;
mod node_name;
mod parse;
mod property;
