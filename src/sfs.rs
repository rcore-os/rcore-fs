use spin::Mutex;
use bit_set::BitSet;
use alloc::{boxed::Box, Vec};
use super::structs::*;
use core::mem::{uninitialized, size_of};
use core::slice;

/// Interface for SFS to read & write
///     TODO: use std::io::{Read, Write}
pub trait Device {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Option<usize>;
}

/// inode for sfs
pub struct Inode {
    /// on-disk inode
    disk_inode: *mut DiskInode,
    /// inode number
    id: u32,
    /// true if inode modified
    dirty: bool,
    /// kill inode if it hits zero
    reclaim_count: u32,
    /// semaphore for din
    mutex: Mutex<()>,
}

/// filesystem for sfs
pub struct SimpleFileSystem {
    /// on-disk superblock
    super_block: SuperBlock,
    /// blocks in use are mared 0
    free_map: BitSet,
    /// true if super/freemap modified
    super_dirty: bool,
    /// buffer for non-block aligned io
//    buffer: u8,
    /// semaphore for fs
//    fs_mutex: Mutex<()>,
    /// semaphore for link/unlink and rename
//    link_mutex: Mutex<()>,
    /// inode list
//    inodes: Vec<Inode>,
    /// device
    device: Mutex<Box<Device>>,
}

impl SimpleFileSystem {
    /// Create a new SFS with device
    pub fn new(mut device: Box<Device>) -> Option<Self> {
        let mut super_block: SuperBlock = unsafe{ uninitialized() };
        let slice = unsafe{ slice::from_raw_parts_mut(
            &mut super_block as *mut SuperBlock as *mut u8, size_of::<SuperBlock>()) };
        if device.read_at(0, slice).is_none() {
            return None;
        }
        if super_block.check() == false {
            return None;
        }

        Some(SimpleFileSystem {
            super_block,
            free_map: BitSet::new(),
            super_dirty: false,
            device: Mutex::new(device),
        })
    }
}