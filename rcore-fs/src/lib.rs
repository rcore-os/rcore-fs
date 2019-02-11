#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]
#![feature(const_str_len)]

extern crate alloc;

mod dirty;
mod util;
pub mod vfs;
pub mod sfs;
pub mod sefs;
pub mod file;
#[cfg(test)]
mod tests;
