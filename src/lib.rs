#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]
#![feature(const_fn)]
#![feature(nll)]
#![feature(extern_crate_item_prelude)]

extern crate alloc;

#[cfg(not(test))]
macro_rules! eprintln {
    () => ();
    ($fmt:expr) => ();
    ($fmt:expr, $($arg:tt)*) => ();
}

mod dirty;
mod util;
mod blocked_device;
mod vfs;
mod sfs;
pub mod file;
mod structs;
#[cfg(test)]
mod tests;

pub use crate::sfs::*;
pub use crate::vfs::*;
pub use crate::blocked_device::BlockedDevice;

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