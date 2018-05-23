#![feature(alloc)]
#![feature(const_fn)]
#![cfg_attr(feature = "ucore", feature(allocator_api, global_allocator, lang_items))]
#![no_std]

#[cfg(any(test, feature = "std"))]
#[macro_use]
extern crate std;
#[macro_use]
extern crate alloc;
extern crate bit_set;
#[cfg(feature = "ucore")]
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate static_assertions;

#[cfg(not(test))]
macro_rules! eprintln {
    () => ();
    ($fmt:expr) => ();
    ($fmt:expr, $($arg:tt)*) => ();
}

mod dirty;
mod vfs;
mod sfs;
mod structs;
#[cfg(feature = "ucore")]
pub mod c_interface;
#[cfg(test)]
mod tests;

pub use sfs::*;
pub use vfs::*;

#[cfg(feature = "ucore")]
#[global_allocator]
pub static UCORE_ALLOCATOR: c_interface::UcoreAllocator = c_interface::UcoreAllocator{};

#[cfg(any(test, feature = "std"))]
pub mod std_impl {
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write, Seek, SeekFrom};
    use super::Device;

    impl Device for File {
        fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize> {
            let offset = offset as u64;
            match self.seek(SeekFrom::Start(offset)) {
                Ok(real_offset) if real_offset == offset => self.read(buf).ok(),
                _ => None,
            }
        }

        fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize> {
            let offset = offset as u64;
            match self.seek(SeekFrom::Start(offset)) {
                Ok(real_offset) if real_offset == offset => self.write(buf).ok(),
                _ => None,
            }
        }
    }
}