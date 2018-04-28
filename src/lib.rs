#![feature(alloc)]
#![no_std]

extern crate spin;
extern crate alloc;

mod vfs;
mod structs;
#[cfg(test)]
mod tests;