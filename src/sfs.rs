use spin::Mutex;
use bit_set::BitSet;
use alloc::{boxed::Box, Vec, BTreeMap, rc::Rc};
use dirty::Dirty;
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
pub struct INode {
    /// on-disk inode
    disk_inode: Dirty<DiskINode>,
    /// inode number
    id: INodeId,
}

type INodeId = usize;

/// filesystem for sfs
pub struct SimpleFileSystem {
    /// on-disk superblock
    super_block: Dirty<SuperBlock>,
    /// blocks in use are mared 0
    free_map: BitSet,
    /// inode list
    inodes: BTreeMap<INodeId, Rc<INode>>,
    /// device
    device: Box<Device>,
}

impl SimpleFileSystem {
    /// Create a new SFS with device
    pub fn new(mut device: Box<Device>) -> Option<Self> {
        let mut super_block: SuperBlock = unsafe{ uninitialized() };
        if device.read_at(0, super_block.as_buf_mut()).is_none() {
            return None;
        }
        if super_block.check() == false {
            return None;
        }

        Some(SimpleFileSystem {
            super_block: Dirty::new(super_block),
            free_map: BitSet::new(),
            inodes: BTreeMap::<INodeId, Rc<INode>>::new(),
            device,
        })
    }
    /// Allocate a block, return block id
    fn alloc_block(&mut self) -> Option<usize> {
        let id = self.free_map.alloc();
        if id.is_some() {
            self.super_block.unused_blocks -= 1;    // will panic if underflow
        }
        id
    }
    /// Free a block
    fn free_block(&mut self, block_id: usize) {
        assert!(!self.free_map.contains(block_id));
        self.free_map.insert(block_id);
        self.super_block.unused_blocks += 1;
    }
    /// Get inode by id
    fn get_inode(&self, id: INodeId) -> Option<Rc<INode>> {
        self.inodes.get(&id).map(|rc| rc.clone())
    }
    /// Write back super block if dirty
    fn sync(&mut self) {
        let SimpleFileSystem {
            ref mut super_block,
            ref mut device,
            ..
        } = self;

        if super_block.dirty() {
            device.write_at(0, super_block.as_buf());
            super_block.sync();
        }
    }
}

trait BitsetAlloc {
    fn alloc(&mut self) -> Option<usize>;
}

impl BitsetAlloc for BitSet {
    fn alloc(&mut self) -> Option<usize> {
        // TODO: more efficient
        let id = (0 .. self.len()).find(|&i| self.contains(i));
        if let Some(id) = id {
            self.remove(id);
        }
        id
    }
}

#[cfg(test)]
mod test {
    use super::*;
}