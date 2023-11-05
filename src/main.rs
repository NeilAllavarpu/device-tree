#![feature(new_uninit)]
#![feature(iter_array_chunks)]
use core::ptr::NonNull;
use std::{env, fs};

fn main() -> Result<(), ()> {
    let dt_path = env::args().nth(1).ok_or(())?;
    let dt_contents = fs::read(dt_path).map_err(|_| ())?;

    let aligned_dt: Box<_> = dt_contents
        .into_iter()
        .array_chunks::<8>()
        .map(u64::from_ne_bytes)
        .collect();

    println!("aligned_dt {:X?}", &aligned_dt[..10]);
    let aligned_dt_addr = NonNull::new(Box::into_raw(aligned_dt)).ok_or(())?;
    let device_tree = unsafe { device_tree::DeviceTree::from_raw(aligned_dt_addr.cast()) };
    drop(unsafe { Box::from_raw(aligned_dt_addr.as_ptr()) });
    println!("{:?}", device_tree);
    Ok(())
}
