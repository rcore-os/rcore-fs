#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]
#![feature(const_str_len)]

extern crate alloc;

pub mod dev;
pub mod dirty;
pub mod file;
pub mod util;
pub mod vfs;
