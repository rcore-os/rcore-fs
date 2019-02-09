#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]
#![feature(const_fn)]
#![feature(const_str_len)]
#![feature(nll)]

extern crate alloc;

#[cfg(not(test))]
#[allow(unused_macros)]
macro_rules! eprintln {
    () => ();
    ($fmt:expr) => ();
    ($fmt:expr, $($arg:tt)*) => ();
}

mod dirty;
mod util;
mod blocked_device;
pub mod vfs;
pub mod sfs;
pub mod file;
mod structs;
#[cfg(test)]
mod tests;

pub use crate::sfs::*;
pub use crate::vfs::*;
pub use crate::blocked_device::BlockedDevice;

#[cfg(any(test, feature = "std"))]
pub mod std_impl {
    use std::fs::File;
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