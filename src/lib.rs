#![feature(alloc)]
#![feature(const_fn)]
#![no_std]

#[cfg(any(test, feature = "std"))]
#[macro_use]
extern crate std;
#[macro_use]
extern crate alloc;
extern crate bit_set;
#[macro_use]
extern crate static_assertions;

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
mod structs;
#[cfg(test)]
mod tests;

pub use sfs::*;
pub use vfs::*;
pub use blocked_device::BlockedDevice;
