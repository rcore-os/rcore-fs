use spin::Mutex;
use bit_set::BitSet;
use alloc::{boxed::Box, Vec, BTreeMap, rc::{Rc, Weak}};
use core::cell::{RefCell, RefMut};
use dirty::Dirty;
use super::structs::*;
use super::vfs;
use core::mem::{uninitialized, size_of};
use core::slice;

/// Interface for SFS to read & write
///     TODO: use std::io::{Read, Write}
pub trait Device {
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;

    // Helper functions

    fn read_block(&mut self, id: BlockId, offset: usize, buf: &mut [u8]) -> Result<(),()> {
        debug_assert!(offset + buf.len() <= BLKSIZE);
        match self.read_at(id * BLKSIZE + offset, buf) {
            Some(len) if len == buf.len() => Ok(()),
            _ => Err(()),
        }
    }
    fn write_block(&mut self, id: BlockId, offset: usize, buf: &[u8]) -> Result<(),()> {
        debug_assert!(offset + buf.len() <= BLKSIZE);
        match self.write_at(id * BLKSIZE + offset, buf) {
            Some(len) if len == buf.len() => Ok(()),
            _ => Err(()),
        }
    }
}

/// inode for sfs
pub struct INode {
    /// on-disk inode
    disk_inode: Dirty<DiskINode>,
    /// inode number
    id: INodeId,
    /// Reference to SFS, used by almost all operations
    fs: Rc<RefCell<SimpleFileSystem>>,
}

impl INode {
    /// Map file block id to disk block id
    fn disk_block_id(&self, file_block_id: BlockId) -> Option<BlockId> {
        match file_block_id {
            id if id >= self.disk_inode.blocks as BlockId =>
                None,
            id if id < NDIRECT =>
                Some(self.disk_inode.direct[id] as BlockId),
            id if id < NDIRECT + BLK_NENTRY => {
                let mut disk_block_id: BlockId = 0;
                self.fs.borrow_mut().device.read_block(
                    self.disk_inode.indirect as usize,
                    ENTRY_SIZE * (id - NDIRECT),
                    disk_block_id.as_buf_mut()
                ).unwrap();
                Some(disk_block_id as BlockId)
            },
            id => unimplemented!("double indirect blocks is not supported"),
        }
    }
}

impl vfs::INode for INode {
    fn open(&mut self, flags: u32) -> Result<(), ()> {
        // Do nothing
        Ok(())
    }
    fn close(&mut self) -> Result<(), ()> {
        self.sync()
    }
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize> {
        let mut fs = self.fs.borrow_mut();

        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
        };

        // Read for each block
        let mut buf_offset = 0usize;
        for BlockRange{block, begin, end} in iter {
            if let Some(disk_block_id) = self.disk_block_id(block) {
                let len = end - begin;
                fs.device.read_block(disk_block_id, begin, &mut buf[buf_offset .. buf_offset + len]);
                buf_offset += len;
            } else {
                // Failed this time
                break;
            }
        }
        Some(buf_offset)
    }
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize> {
        let mut fs = self.fs.borrow_mut();

        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
        };

        // Read for each block
        let mut buf_offset = 0usize;
        for BlockRange{block, begin, end} in iter {
            if let Some(disk_block_id) = self.disk_block_id(block) {
                let len = end - begin;
                fs.device.write_block(disk_block_id, begin, &buf[buf_offset .. buf_offset + len]);
                buf_offset += len;
            } else {
                // Failed this time
                break;
            }
        }
        Some(buf_offset)
    }
    fn sync(&mut self) -> Result<(), ()> {
        if self.disk_inode.dirty() {
            let mut fs = self.fs.borrow_mut();
            fs.device.write_block(self.id, 0, self.disk_inode.as_buf())?;
            self.disk_inode.sync();
        }
        Ok(())
    }
}

/// Given a range and iterate sub-range for each block
struct BlockIter {
    begin: usize,
    end: usize,
}

struct BlockRange {
    block: BlockId,
    begin: usize,
    end: usize,
}

impl Iterator for BlockIter {
    type Item = BlockRange;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.begin >= self.end {
            return None;
        }
        let block = self.begin / BLKSIZE;
        let begin = self.begin % BLKSIZE;
        let end = if block == self.end / BLKSIZE {self.end % BLKSIZE} else {BLKSIZE};
        self.begin += end - begin;
        Some(BlockRange {block, begin, end})
    }
}


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
        if device.read_at(BLKN_SUPER * BLKSIZE, super_block.as_buf_mut()).is_none() {
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

impl vfs::FileSystem for SimpleFileSystem {
    type INode = INode;

    fn sync(&mut self) -> Result<(), ()> {
        unimplemented!()
    }

    fn root_inode(&mut self) -> Rc<INode> {
        unimplemented!()
    }

    fn unmount(&mut self) -> Result<(), ()> {
        unimplemented!()
    }

    fn cleanup(&mut self) {
        unimplemented!()
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