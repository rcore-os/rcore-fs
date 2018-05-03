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

    fn read_block(&mut self, id: BlockId, offset: usize, buf: &mut [u8]) -> vfs::Result<()> {
        debug_assert!(offset + buf.len() <= BLKSIZE);
        match self.read_at(id * BLKSIZE + offset, buf) {
            Some(len) if len == buf.len() => Ok(()),
            _ => Err(()),
        }
    }
    fn write_block(&mut self, id: BlockId, offset: usize, buf: &[u8]) -> vfs::Result<()> {
        debug_assert!(offset + buf.len() <= BLKSIZE);
        match self.write_at(id * BLKSIZE + offset, buf) {
            Some(len) if len == buf.len() => Ok(()),
            _ => Err(()),
        }
    }
}

trait DeviceExt: Device {
    /// Load struct `T` from given block in device
    fn load_struct<T: AsBuf>(&mut self, id: BlockId) -> T {
        let mut s: T = unsafe { uninitialized() };
        self.read_block(id, 0, s.as_buf_mut()).unwrap();
        s
    }
}
impl DeviceExt for Device {}

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
    fn get_disk_block_id(&self, file_block_id: BlockId) -> Option<BlockId> {
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
    fn set_disk_block_id(&mut self, file_block_id: BlockId, disk_block_id: BlockId) -> vfs::Result<()> {
        match file_block_id {
            id if id >= self.disk_inode.blocks as BlockId =>
                Err(()),
            id if id < NDIRECT => {
                self.disk_inode.direct[id] = disk_block_id as u32;
                Ok(())
            },
            id if id < NDIRECT + BLK_NENTRY => {
                let disk_block_id = disk_block_id as u32;
                let fs = self.fs.upgrade().unwrap();
                fs.borrow_mut().device.write_block(
                    self.disk_inode.indirect as usize,
                    ENTRY_SIZE * (id - NDIRECT),
                    disk_block_id.as_buf(),
                ).unwrap();
                Ok(())
            }
            id => unimplemented!("double indirect blocks is not supported"),
        }
    }
    /// Only for Dir
    fn get_file_inode_id(&mut self, name: &'static str) -> Option<INodeId> {
        (0 .. self.disk_inode.blocks)
            .map(|i| {
                use vfs::INode;
                let mut entry: DiskEntry = unsafe {uninitialized()};
                self.read_at(i as usize * BLKSIZE, entry.as_buf_mut()).unwrap();
                entry
            })
            .find(|entry| entry.name.as_ref() == name)
            .map(|entry| entry.id as INodeId)
    }
    /// Init dir content. Insert 2 init entries.
    fn init_dir(&mut self, parent: INodeId) -> vfs::Result<()> {
        use vfs::INode;
        // Insert entries: '.' '..'
        self.resize(BLKSIZE * 2).unwrap();
        self.write_at(0, DiskEntry {
            id: parent as u32,
            name: Str256::from(".."),
        }.as_buf()).unwrap();
        let id = self.id as u32;
        self.write_at(0, DiskEntry {
            id,
            name: Str256::from("."),
        }.as_buf()).unwrap();
        Ok(())
    }
    fn clean_at(&mut self, begin: usize, end: usize) -> vfs::Result<()> {
        let fs = self.fs.upgrade().unwrap();

        let iter = BlockIter { begin, end };
        for BlockRange { block, begin, end } in iter {
            static ZEROS: [u8; BLKSIZE] = [0; BLKSIZE];
            let disk_block_id = self.get_disk_block_id(block).unwrap();
            fs.borrow_mut().device.write_block(disk_block_id, begin, &ZEROS[begin..end]).unwrap();
        }
        Ok(())
    }
}

impl vfs::INode for INode {
    fn open(&mut self, flags: u32) -> vfs::Result<()> {
        // Do nothing
        Ok(())
    }
    fn close(&mut self) -> vfs::Result<()> {
        self.sync()
    }
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> vfs::Result<usize> {
        let fs = self.fs.upgrade().unwrap();

        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
        };

        // Read for each block
        let mut buf_offset = 0usize;
        for BlockRange { block, begin, end } in iter {
            let disk_block_id = self.get_disk_block_id(block).unwrap();
            let len = end - begin;
            fs.borrow_mut().device.read_block(disk_block_id, begin, &mut buf[buf_offset..buf_offset + len]).unwrap();
            buf_offset += len;
        }
        Ok(buf_offset)
    }
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> vfs::Result<usize> {
        let fs = self.fs.upgrade().unwrap();

        let iter = BlockIter {
            begin: offset,
            end: offset + buf.len(),
        };

        // Write for each block
        let mut buf_offset = 0usize;
        for BlockRange { block, begin, end } in iter {
            let disk_block_id = self.get_disk_block_id(block).unwrap();
            let len = end - begin;
            fs.borrow_mut().device.write_block(disk_block_id, begin, &buf[buf_offset..buf_offset + len]).unwrap();
            buf_offset += len;
        }
        Ok(buf_offset)
    }
    fn info(&mut self) -> vfs::Result<vfs::FileInfo> {
        Ok(vfs::FileInfo {
            size: self.disk_inode.size as usize,
            mode: 0,
            type_: vfs::FileType::from(self.disk_inode.type_.clone()),
        })
    }
    fn sync(&mut self) -> vfs::Result<()> {
        if self.disk_inode.dirty() {
            let fs = self.fs.upgrade().unwrap();
            fs.borrow_mut().device.write_block(self.id, 0, self.disk_inode.as_buf()).unwrap();
            self.disk_inode.sync();
        }
        Ok(())
    }
    fn resize(&mut self, len: usize) -> vfs::Result<()> {
        if len > MAX_FILE_SIZE {
            return Err(());
        }
        let blocks = ((len + BLKSIZE - 1) / BLKSIZE) as u32;
        use core::cmp::{Ord, Ordering};
        match blocks.cmp(&self.disk_inode.blocks) {
            Ordering::Equal => {},  // Do nothing
            Ordering::Greater => {
                let fs = self.fs.upgrade().unwrap();
                let old_blocks = self.disk_inode.blocks;
                self.disk_inode.blocks = blocks;
                // allocate indirect block if need
                if old_blocks < NDIRECT as u32 && blocks >= NDIRECT as u32 {
                    self.disk_inode.indirect = fs.borrow_mut().alloc_block().unwrap() as u32;
                }
                // allocate extra blocks
                for i in old_blocks .. blocks {
                    let disk_block_id = fs.borrow_mut().alloc_block().expect("no more space");
                    self.set_disk_block_id(i as usize, disk_block_id).unwrap();
                }
                // clean up
                let old_size = self.disk_inode.size as usize;
                self.clean_at(old_size, len).unwrap();
            },
            Ordering::Less => {
                let fs = self.fs.upgrade().unwrap();
                // free extra blocks
                for i in blocks .. self.disk_inode.blocks {
                    let disk_block_id = self.get_disk_block_id(i as usize).unwrap();
                    fs.borrow_mut().free_block(disk_block_id);
                }
                // free indirect block if need
                if blocks < NDIRECT as u32 && self.disk_inode.blocks >= NDIRECT as u32 {
                    fs.borrow_mut().free_block(self.disk_inode.indirect as usize);
                    self.disk_inode.indirect = 0;
                }
                self.disk_inode.blocks = blocks;
            },
        }
        self.disk_inode.size = len as u32;
        Ok(())
    }
    fn create(&mut self, name: &'static str) -> vfs::Result<Ptr<vfs::INode>> {
        let fs = self.fs.upgrade().unwrap();
        let info = self.info().unwrap();
        assert_eq!(info.type_, vfs::FileType::Dir);
        assert_eq!(info.size % BLKSIZE, 0);

        // Ensure the name is not exist
        assert!(self.get_file_inode_id(name).is_none(), "file name exist");

        // Create new INode
        let inode = fs.borrow_mut().new_inode_file().unwrap();

        // Write new entry
        let entry = DiskEntry {
            id: inode.borrow().id as u32,
            name: Str256::from(name),
        };
        self.resize(info.size + BLKSIZE).unwrap();
        self.write_at(info.size, entry.as_buf()).unwrap();

        Ok(inode)
    }
    fn loopup(&mut self, path: &'static str) -> vfs::Result<Ptr<vfs::INode>> {
        let fs = self.fs.upgrade().unwrap();
        let info = self.info().unwrap();
        assert_eq!(info.type_, vfs::FileType::Dir);
        assert_eq!(info.size % BLKSIZE, 0);

        let (name, rest_path) = match path.find('/') {
            None => (path, ""),
            Some(pos) => (&path[0..pos], &path[pos+1..]),
        };
        let inode_id = self.get_file_inode_id(name);
        if inode_id.is_none() {
            return Err(());
        }
        let inode = fs.borrow_mut().get_inode(inode_id.unwrap());

        let type_ = inode.borrow().disk_inode.type_;
        match type_ {
            FileType::File => if rest_path == "" {Ok(inode)} else {Err(())},
            FileType::Dir => inode.borrow_mut().loopup(rest_path),
            _ => unimplemented!(),
        }
    }
}

impl Drop for INode {
    /// Auto sync when drop
    fn drop(&mut self) {
        use vfs::INode;
        self.sync().expect("failed to sync");
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
        let super_block = device.load_struct::<SuperBlock>(BLKN_SUPER);
        if super_block.check() == false {
            return None;
        }
        let free_map = device.load_struct::<[u8; BLKSIZE]>(BLKN_FREEMAP);

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
            info: Str32::from("simple file system"),
        };
        let free_map = {
            let mut bitset = BitSet::with_capacity(BLKBITS);
            for i in 3 .. blocks {
                bitset.insert(i);
            }
            bitset
        };

        let sfs = SimpleFileSystem {
            super_block: Dirty::new_dirty(super_block),
            free_map: Dirty::new_dirty(free_map),
            inodes: BTreeMap::<INodeId, Ptr<INode>>::new(),
            device,
            self_ptr: Weak::default(),
        }.wrap();

        // Init root INode
        let inode = Rc::new(RefCell::new(INode {
            disk_inode: Dirty::new_dirty(DiskINode::new_dir()),
            id: BLKN_ROOT,
            fs: Rc::downgrade(&sfs),
        }));
        inode.borrow_mut().init_dir(BLKN_ROOT).unwrap();
        {
            use vfs::INode;
            inode.borrow_mut().sync().unwrap();
        }
        sfs.borrow_mut().inodes.insert(BLKN_ROOT, inode);

        sfs
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
            let disk_inode = self.device.load_struct::<DiskINode>(id);
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
    /// Create a new INode file
    fn new_inode_file(&mut self) -> vfs::Result<Ptr<INode>> {
        let id = self.alloc_block().unwrap();
        Ok(Rc::new(RefCell::new(INode {
            disk_inode: Dirty::new_dirty(DiskINode::new_file()),
            id,
            fs: self.self_ptr.clone(),
        })))
    }
    /// Create a new INode dir
    fn new_inode_dir(&mut self, parent: INodeId) -> vfs::Result<Ptr<INode>> {
        let id = self.alloc_block().unwrap();
        let mut inode = INode {
            disk_inode: Dirty::new_dirty(DiskINode::new_dir()),
            id,
            fs: self.self_ptr.clone(),
        };
        inode.init_dir(parent).unwrap();

        Ok(Rc::new(RefCell::new(inode)))
    }
}

impl vfs::FileSystem for SimpleFileSystem {
    type INode = INode;

    /// Write back super block if dirty
    fn sync(&mut self) -> vfs::Result<()> {
        let SimpleFileSystem {
            ref mut super_block,
            ref mut device,
            ref mut free_map,
            ref mut inodes,
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
        for inode in inodes.values() {
            use vfs::INode;
            inode.borrow_mut().sync().unwrap();
        }
        Ok(())
    }

    fn root_inode(&mut self) -> Ptr<INode> {
        self.get_inode(BLKN_ROOT)
    }

    fn unmount(&mut self) -> vfs::Result<()> {
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

impl From<FileType> for vfs::FileType {
    fn from(t: FileType) -> Self {
        match t {
            FileType::File => vfs::FileType::File,
            FileType::Dir => vfs::FileType::Dir,
            _ => panic!("unknown file type"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
}