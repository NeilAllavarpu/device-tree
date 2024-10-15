#![feature(iter_array_chunks)]
#![feature(iter_intersperse)]

use core::mem;
use std::{
    env,
    error::Error,
    fs,
    io::{stdout, Write},
};

use device_tree::node::Node;

fn main() -> Result<(), Box<dyn Error>> {
    let dt_path = env::args().nth(1).ok_or("Missing path to DTB")?;
    let mut dt_contents = fs::read(dt_path)?;
    while dt_contents.len() % mem::size_of::<u64>() != 0 {
        dt_contents.push(0)
    }

    let aligned_dt: Box<_> = dt_contents
        .into_iter()
        .array_chunks::<{ mem::size_of::<u64>() / mem::size_of::<u8>() }>()
        .map(u64::from_ne_bytes)
        .collect();
    // let aligned_dt_addr =
    // NonNull::new(Box::into_raw(aligned_dt)).expect("Boxes should never be null pointers");
    let device_tree = device_tree::dtb::DeviceTree::from_bytes(&aligned_dt).unwrap();

    let root = device_tree.root();
    println!(
        "{:#X?}",
        device_tree
            .root()
            .find_str("/soc".as_bytes())
            .unwrap()
            .children()
            .iter()
            .map(|(x, _)| x)
            .collect::<Box<_>>() // .properties()
                                 // .iter() // .map(|(a, b)| { (a) })
                                 // .collect::<Vec<_>>()
    );

    println!("{:X?}", root);
    print!("$ ");
    stdout().flush().unwrap();

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
