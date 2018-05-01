use spin::Mutex;
use bit_set::BitSet;
use alloc::{boxed::Box, Vec, BTreeMap, rc::{Rc, Weak}};
use core::cell::{RefCell, RefMut};
use dirty::Dirty;
use super::structs::*;
use super::vfs;
use core::mem::{uninitialized, size_of};
use core::slice;
use core::fmt::{Debug, Formatter, Error};

/// Interface for SFS to read & write
///     TODO: use std::io::{Read, Write}
pub trait Device {
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;

    // Helper functions

    fn read_block(&mut self, id: BlockId, offset: usize, buf: &mut [u8]) -> Result<(), ()> {
        debug_assert!(offset + buf.len() <= BLKSIZE);
        match self.read_at(id * BLKSIZE + offset, buf) {
            Some(len) if len == buf.len() => Ok(()),
            _ => Err(()),
        }
    }
    fn write_block(&mut self, id: BlockId, offset: usize, buf: &[u8]) -> Result<(), ()> {
        debug_assert!(offset + buf.len() <= BLKSIZE);
        match self.write_at(id * BLKSIZE + offset, buf) {
            Some(len) if len == buf.len() => Ok(()),
            _ => Err(()),
        }
    }
}

/// Load struct `T` from given block in device
///     Workaround: It should be inside the trait `Device`. But that would cause compile error.
fn load_struct<T: AsBuf>(device: &mut Device, id: BlockId) -> T {
    let mut s: T = unsafe { uninitialized() };
    device.read_block(id, 0, s.as_buf_mut()).unwrap();
    s
}

type Ptr<T> = Rc<RefCell<T>>;
type WeakPtr<T> = Weak<RefCell<T>>;

/// inode for sfs
pub struct INode {
    /// on-disk inode
    disk_inode: Dirty<DiskINode>,
    /// inode number
    id: INodeId,
    /// Weak reference to SFS, used by almost all operations
    fs: WeakPtr<SimpleFileSystem>,
}

impl Debug for INode {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{:?}", *self.disk_inode)
    }
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
                let mut disk_block_id: u32 = 0;
                let fs = self.fs.upgrade().unwrap();
                fs.borrow_mut().device.read_block(
                    self.disk_inode.indirect as usize,
                    ENTRY_SIZE * (id - NDIRECT),
                    disk_block_id.as_buf_mut(),
                ).unwrap();
                Some(disk_block_id as BlockId)
            }
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
        let fs = self.fs.upgrade().unwrap();

        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
        };

        // Read for each block
        let mut buf_offset = 0usize;
        for BlockRange { block, begin, end } in iter {
            if let Some(disk_block_id) = self.disk_block_id(block) {
                let len = end - begin;
                fs.borrow_mut().device.read_block(disk_block_id, begin, &mut buf[buf_offset..buf_offset + len]).unwrap();
                buf_offset += len;
            } else {
                // Failed this time
                break;
            }
        }
        Some(buf_offset)
    }
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize> {
        let fs = self.fs.upgrade().unwrap();

        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
        };

        // Read for each block
        let mut buf_offset = 0usize;
        for BlockRange { block, begin, end } in iter {
            if let Some(disk_block_id) = self.disk_block_id(block) {
                let len = end - begin;
                fs.borrow_mut().device.write_block(disk_block_id, begin, &buf[buf_offset..buf_offset + len]).unwrap();
                buf_offset += len;
            } else {
                // Failed this time
                break;
            }
        }
        Some(buf_offset)
    }
    fn info(&mut self) -> Result<vfs::FileInfo, ()> {
        if self.disk_inode.type_ != FileType::File {
            return Err(());
        }
        Ok(vfs::FileInfo {
            size: self.disk_inode.size as usize,
            mode: 0,
        })
    }
    fn sync(&mut self) -> Result<(), ()> {
        if self.disk_inode.dirty() {
            let fs0 = self.fs.upgrade().unwrap();
            let mut fs = fs0.borrow_mut();
            fs.device.write_block(self.id, 0, self.disk_inode.as_buf())?;
            self.disk_inode.sync();
        }
        Ok(())
    }
    fn type_(&self) -> Result<u32, ()> {
        Ok(self.disk_inode.type_.clone() as u32)
    }
}

/// Given a range and iterate sub-range for each block
struct BlockIter {
    begin: usize,
    end: usize,
}

#[derive(Debug)]
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
        let end = if block == self.end / BLKSIZE { self.end % BLKSIZE } else { BLKSIZE };
        self.begin += end - begin;
        Some(BlockRange { block, begin, end })
    }
}


/// filesystem for sfs
pub struct SimpleFileSystem {
    /// on-disk superblock
    super_block: Dirty<SuperBlock>,
    /// blocks in use are mared 0
    free_map: Dirty<BitSet>,
    /// inode list
    inodes: BTreeMap<INodeId, Ptr<INode>>,
    /// device
    device: Box<Device>,
    /// Pointer to self, used by INodes
    self_ptr: WeakPtr<SimpleFileSystem>,
}

impl SimpleFileSystem {
    /// Load SFS from device
    pub fn open(mut device: Box<Device>) -> Option<Ptr<Self>> {
        let super_block = load_struct::<SuperBlock>(device.as_mut(), BLKN_SUPER);
        if super_block.check() == false {
            return None;
        }
        let free_map = load_struct::<[u8; BLKSIZE]>(device.as_mut(), BLKN_FREEMAP);

        Some(SimpleFileSystem {
            super_block: Dirty::new(super_block),
            free_map: Dirty::new(BitSet::from_bytes(&free_map)),
            inodes: BTreeMap::<INodeId, Ptr<INode>>::new(),
            device,
            self_ptr: Weak::default(),
        }.wrap())
    }
    /// Create a new SFS on blank disk
    pub fn create(mut device: Box<Device>, space: usize) -> Ptr<Self> {
        let blocks = (space / BLKSIZE).min(BLKBITS);
        assert!(blocks >= 16, "space too small");

        let super_block = SuperBlock {
            magic: MAGIC,
            blocks: blocks as u32,
            unused_blocks: blocks as u32 - 3,
            info: Str32::from_slice(b"simple file system"),
        };
        let mut free_map = BitSet::with_capacity(BLKBITS);
        for i in 3 .. blocks {
            free_map.insert(i);
        }
        SimpleFileSystem {
            super_block: Dirty::new_dirty(super_block),
            free_map: Dirty::new_dirty(free_map),
            inodes: BTreeMap::<INodeId, Ptr<INode>>::new(),
            device,
            self_ptr: Weak::default(),
        }.wrap()
    }
    /// Wrap pure SimpleFileSystem with Rc<RefCell<...>>
    /// Used in constructors
    fn wrap(self) -> Ptr<Self> {
        let mut fs = Rc::new(RefCell::new(self));
        fs.borrow_mut().self_ptr = Rc::downgrade(&fs);
        fs
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

    /// Get inode by id. Load if not in memory.
    /// ** Must ensure it's a valid INode **
    fn get_inode(&mut self, id: INodeId) -> Ptr<INode> {
        assert!(!self.free_map.contains(id));

        // Load if not in memory.
        if !self.inodes.contains_key(&id) {
            let disk_inode = load_struct::<DiskINode>(self.device.as_mut(), id);
            let inode = Rc::new(RefCell::new(INode {
                disk_inode: Dirty::new(disk_inode),
                id,
                fs: self.self_ptr.clone(),
            }));
            self.inodes.insert(id, inode.clone());
            inode
        } else {
            self.inodes.get(&id).unwrap().clone()
        }
    }
}

impl vfs::FileSystem for SimpleFileSystem {
    type INode = INode;

    /// Write back super block if dirty
    fn sync(&mut self) -> Result<(), ()> {
        let SimpleFileSystem {
            ref mut super_block,
            ref mut device,
            ref mut free_map,
            ..
        } = self;

        if super_block.dirty() {
            device.write_at(BLKSIZE * BLKN_SUPER, super_block.as_buf()).unwrap();
            super_block.sync();
        }
        if free_map.dirty() {
            device.write_at(BLKSIZE * BLKN_FREEMAP, free_map.as_buf()).unwrap();
            free_map.sync();
        }
        Ok(())
    }

    fn root_inode(&mut self) -> Ptr<INode> {
        self.get_inode(BLKN_ROOT)
    }

    fn unmount(&mut self) -> Result<(), ()> {
        unimplemented!()
    }

    fn cleanup(&mut self) {
        unimplemented!()
    }
}

impl Drop for SimpleFileSystem {
    /// Auto sync when drop
    fn drop(&mut self) {
        use vfs::FileSystem;
        self.sync().expect("failed to sync");
    }
}

trait BitsetAlloc {
    fn alloc(&mut self) -> Option<usize>;
}

impl BitsetAlloc for BitSet {
    fn alloc(&mut self) -> Option<usize> {
        // TODO: more efficient
        let id = (0..self.len()).find(|&i| self.contains(i));
        if let Some(id) = id {
            self.remove(id);
        }
        id
    }
}

impl AsBuf for BitSet {
    fn as_buf(&self) -> &[u8] {
        let slice = self.get_ref().storage();
        unsafe{ slice::from_raw_parts(slice as *const _ as *const u8, slice.len() * 4) }
    }
    fn as_buf_mut(&mut self) -> &mut [u8] {
        let slice = self.get_ref().storage();
        unsafe{ slice::from_raw_parts_mut(slice as *const _ as *mut u8, slice.len() * 4) }
    }
}

impl AsBuf for [u8; BLKSIZE] {}

#[cfg(test)]
mod test {
    use super::*;
}