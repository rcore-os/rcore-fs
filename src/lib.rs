#![feature(alloc)]
#![feature(const_fn)]
#![cfg_attr(feature = "ucore", feature(allocator_api, global_allocator, lang_items))]
#![no_std]

#[cfg(test)]
#[macro_use]
extern crate std;
extern crate spin;
extern crate alloc;
extern crate bit_set;

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

#[cfg(feature = "ucore")]
#[global_allocator]
pub static UCORE_ALLOCATOR: c_interface::UcoreAllocator = {
    extern {
        fn kmalloc(size: usize) -> *mut u8;
        fn kfree(ptr: *mut u8);
    }
    c_interface::UcoreAllocator{malloc: kmalloc, free: kfree}
};