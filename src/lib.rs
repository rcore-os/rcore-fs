#![feature(alloc)]
#![feature(const_fn)]
#![cfg_attr(target_arch = "riscv", feature(match_default_bindings))]
#![no_std]

#[cfg(any(test, feature = "std"))]
#[macro_use]
extern crate std;
#[macro_use]
extern crate alloc;
extern crate bit_vec;
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
