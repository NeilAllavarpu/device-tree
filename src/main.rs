#![feature(iter_array_chunks)]
#![feature(iter_intersperse)]

use core::mem;
use core::ptr::NonNull;
use std::{
    env,
    error::Error,
    fs,
    io::{stdin, stdout, BufRead, Write},
};

fn main() -> Result<(), Box<dyn Error>> {
    let device_tree = {
        let dt_path = env::args().nth(1).ok_or("Missing path to DTB")?;
        let dt_contents = fs::read(dt_path)?;

        let aligned_dt: Box<_> = dt_contents
            .into_iter()
            .array_chunks::<{ mem::size_of::<u64>() / mem::size_of::<u8>() }>()
            .map(u64::from_ne_bytes)
            .collect();

        let aligned_dt_addr =
            NonNull::new(Box::into_raw(aligned_dt)).expect("Boxes should never be null pointers");
        let device_tree =
            unsafe { device_tree::dtb::DeviceTree::from_raw(aligned_dt_addr.cast()) }?;
        drop(unsafe { Box::from_raw(aligned_dt_addr.as_ptr()) });
        device_tree
    };

    let root = device_tree.get_root();

    println!("{:?}", device_tree.boot_cpu());
    // print!("$ ");
    // stdout().flush().unwrap();

    // for line in stdin().lock().lines() {
    //     let line = line?;

    //     let mut arguments = line.split_whitespace();
    //     match arguments.next() {
    //         Some("ls") => {
    //             println!(
    //                 "{}",
    //                 root.get_children()
    //                     .map(|(name, address)| if let Some(address) = address {
    //                         format!("{name}@{address}")
    //                     } else {
    //                         format!("{name}")
    //                     })
    //                     .collect::<Box<_>>()
    //                     .join(" ")
    //             );
    //         }
    //         Some(command) => eprintln!("Invalid command: {command}"),
    //         None => break,
    //     }
    //     print!("$ ");
    //     stdout().flush().unwrap();
    // }

    Ok(())
}
