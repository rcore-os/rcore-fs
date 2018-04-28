#![feature(alloc)]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;

extern crate spin;
extern crate alloc;
extern crate bit_set;

mod sfs;
mod structs;
#[cfg(test)]
mod tests;