use spin::Mutex;
use bit_set::BitSet;
use alloc::{boxed::Box, Vec, BTreeMap, rc::{Rc, Weak}, String};
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
}

trait DeviceExt: Device {
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
    /// Load struct `T` from given block in device
    fn load_struct<T: AsBuf>(&mut self, id: BlockId) -> T {
        let mut s: T = unsafe { uninitialized() };
        self.read_block(id, 0, s.as_buf_mut()).unwrap();
        s
    }
}
impl DeviceExt for Device {}

type Ptr<T> = Rc<RefCell<T>>;

/// inode for sfs
pub struct INode {
    /// on-disk inode
    disk_inode: Dirty<DiskINode>,
    /// inode number
    id: INodeId,
    /// Weak reference to SFS, used by almost all operations
    fs: Weak<SimpleFileSystem>,
}

impl Debug for INode {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "INode {{ id: {}, disk: {:?} }}", self.id, self.disk_inode)
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
                fs.device.borrow_mut().read_block(
                    self.disk_inode.indirect as usize,
                    ENTRY_SIZE * (id - NDIRECT),
                    disk_block_id.as_buf_mut(),
                ).unwrap();
                Some(disk_block_id as BlockId)
            }
            _ => unimplemented!("double indirect blocks is not supported"),
        }
    }
    fn set_disk_block_id(&mut self, file_block_id: BlockId, disk_block_id: BlockId) -> vfs::Result<()> {
        match file_block_id {
            id if id >= self.disk_inode.blocks as BlockId =>
                Err(()),
            id if id < NDIRECT => {
                self.disk_inode.direct[id] = disk_block_id as u32;
                Ok(())
            }
            id if id < NDIRECT + BLK_NENTRY => {
                let disk_block_id = disk_block_id as u32;
                let fs = self.fs.upgrade().unwrap();
                fs.device.borrow_mut().write_block(
                    self.disk_inode.indirect as usize,
                    ENTRY_SIZE * (id - NDIRECT),
                    disk_block_id.as_buf(),
                ).unwrap();
                Ok(())
            }
            _ => unimplemented!("double indirect blocks is not supported"),
        }
    }
    /// Only for Dir
    fn get_file_inode_id(&self, name: &'static str) -> Option<INodeId> {
        (0..self.disk_inode.blocks)
            .map(|i| {
                use vfs::INode;
                let mut entry: DiskEntry = unsafe { uninitialized() };
                self._read_at(i as usize * BLKSIZE, entry.as_buf_mut()).unwrap();
                entry
            })
            .find(|entry| entry.name.as_ref() == name)
            .map(|entry| entry.id as INodeId)
    }
    /// Init dir content. Insert 2 init entries.
    fn init_dir(&mut self, parent: INodeId) -> vfs::Result<()> {
        use vfs::INode;
        // Insert entries: '.' '..'
        self._resize(BLKSIZE * 2).unwrap();
        self._write_at(BLKSIZE * 1, DiskEntry {
            id: parent as u32,
            name: Str256::from(".."),
        }.as_buf()).unwrap();
        let id = self.id as u32;
        self._write_at(BLKSIZE * 0, DiskEntry {
            id,
            name: Str256::from("."),
        }.as_buf()).unwrap();
        Ok(())
    }
    /// Resize content size, no matter what type it is.
    fn _resize(&mut self, len: usize) -> vfs::Result<()> {
        assert!(len <= MAX_FILE_SIZE, "file size exceed limit");
        let blocks = ((len + BLKSIZE - 1) / BLKSIZE) as u32;
        use core::cmp::{Ord, Ordering};
        match blocks.cmp(&self.disk_inode.blocks) {
            Ordering::Equal => {}  // Do nothing
            Ordering::Greater => {
                let fs = self.fs.upgrade().unwrap();
                let old_blocks = self.disk_inode.blocks;
                self.disk_inode.blocks = blocks;
                // allocate indirect block if need
                if old_blocks < NDIRECT as u32 && blocks >= NDIRECT as u32 {
                    self.disk_inode.indirect = fs.alloc_block().unwrap() as u32;
                }
                // allocate extra blocks
                for i in old_blocks..blocks {
                    let disk_block_id = fs.alloc_block().expect("no more space");
                    self.set_disk_block_id(i as usize, disk_block_id).unwrap();
                }
                // clean up
                let old_size = self.disk_inode.size as usize;
                self.disk_inode.size = len as u32;
                self._clean_at(old_size, len).unwrap();
            }
            Ordering::Less => {
                let fs = self.fs.upgrade().unwrap();
                // free extra blocks
                for i in blocks..self.disk_inode.blocks {
                    let disk_block_id = self.get_disk_block_id(i as usize).unwrap();
                    fs.free_block(disk_block_id);
                }
                // free indirect block if need
                if blocks < NDIRECT as u32 && self.disk_inode.blocks >= NDIRECT as u32 {
                    fs.free_block(self.disk_inode.indirect as usize);
                    self.disk_inode.indirect = 0;
                }
                self.disk_inode.blocks = blocks;
            }
        }
        self.disk_inode.size = len as u32;
        Ok(())
    }
    /// Read/Write content, no matter what type it is
    fn _io_at<F>(&self, begin: usize, end: usize, mut f: F) -> vfs::Result<usize>
        where F: FnMut(RefMut<Box<Device>>, &BlockRange, usize)
    {
        let fs = self.fs.upgrade().unwrap();

        let size = match self.disk_inode.type_ {
            FileType::Dir => self.disk_inode.blocks as usize * BLKSIZE,
            FileType::File => self.disk_inode.size as usize,
            _ => unimplemented!(),
        };
        let iter = BlockIter {
            begin: size.min(begin),
            end: size.min(end),
        };

        // For each block
        let mut buf_offset = 0usize;
        for mut range in iter {
            range.block = self.get_disk_block_id(range.block).unwrap();
            f(fs.device.borrow_mut(), &range, buf_offset);
            buf_offset += range.len();
        }
        Ok(buf_offset)
    }
    /// Read content, no matter what type it is
    fn _read_at(&self, offset: usize, buf: &mut [u8]) -> vfs::Result<usize> {
        self._io_at(offset, offset + buf.len(), |mut device, range, offset| {
            device.read_block(range.block, range.begin, &mut buf[offset..offset + range.len()]).unwrap()
        })
    }
    /// Write content, no matter what type it is
    fn _write_at(&self, offset: usize, buf: &[u8]) -> vfs::Result<usize> {
        self._io_at(offset, offset + buf.len(), |mut device, range, offset| {
            device.write_block(range.block, range.begin, &buf[offset..offset + range.len()]).unwrap()
        })
    }
    /// Clean content, no matter what type it is
    fn _clean_at(&self, begin: usize, end: usize) -> vfs::Result<()> {
        static ZEROS: [u8; BLKSIZE] = [0; BLKSIZE];
        self._io_at(begin, end, |mut device, range, _| {
            device.write_block(range.block, range.begin, &ZEROS[..range.len()]).unwrap()
        }).unwrap();
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
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> vfs::Result<usize> {
        assert_eq!(self.disk_inode.type_, FileType::File, "read_at is only available on file");
        self._read_at(offset, buf)
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> vfs::Result<usize> {
        assert_eq!(self.disk_inode.type_, FileType::File, "write_at is only available on file");
        self._write_at(offset, buf)
    }
    fn info(&self) -> vfs::Result<vfs::FileInfo> {
        Ok(vfs::FileInfo {
            size: self.disk_inode.size as usize,
            mode: 0,
            type_: vfs::FileType::from(self.disk_inode.type_.clone()),
            blocks: self.disk_inode.blocks as usize,
        })
    }
    fn sync(&mut self) -> vfs::Result<()> {
        if self.disk_inode.dirty() {
            let fs = self.fs.upgrade().unwrap();
            fs.device.borrow_mut().write_block(self.id, 0, self.disk_inode.as_buf()).unwrap();
            self.disk_inode.sync();
        }
        Ok(())
    }
    fn resize(&mut self, len: usize) -> vfs::Result<()> {
        assert_eq!(self.disk_inode.type_, FileType::File, "resize is only available on file");
        self._resize(len)
    }
    fn create(&mut self, name: &'static str, type_: vfs::FileType) -> vfs::Result<Ptr<vfs::INode>> {
        let fs = self.fs.upgrade().unwrap();
        let info = self.info().unwrap();
        assert_eq!(info.type_, vfs::FileType::Dir);

        // Ensure the name is not exist
        assert!(self.get_file_inode_id(name).is_none(), "file name exist");

        // Create new INode
        let inode = match type_ {
            vfs::FileType::File => fs.new_inode_file().unwrap(),
            vfs::FileType::Dir => fs.new_inode_dir(self.id).unwrap(),
        };

        // Write new entry
        let entry = DiskEntry {
            id: inode.borrow().id as u32,
            name: Str256::from(name),
        };
        self._resize(info.size + BLKSIZE).unwrap();
        self._write_at(info.size, entry.as_buf()).unwrap();

        Ok(inode)
    }
    fn lookup(&self, path: &'static str) -> vfs::Result<Ptr<vfs::INode>> {
        let fs = self.fs.upgrade().unwrap();
        let info = self.info().unwrap();
        assert_eq!(info.type_, vfs::FileType::Dir);

        let (name, rest_path) = match path.find('/') {
            None => (path, ""),
            Some(pos) => (&path[0..pos], &path[pos + 1..]),
        };
        let inode_id = self.get_file_inode_id(name);
        if inode_id.is_none() {
            return Err(());
        }
        let inode = fs.get_inode(inode_id.unwrap());

        let type_ = inode.borrow().disk_inode.type_;
        match type_ {
            FileType::File => if rest_path == "" { Ok(inode) } else { Err(()) },
            FileType::Dir => if rest_path == "" { Ok(inode) } else { inode.borrow().lookup(rest_path) },
            _ => unimplemented!(),
        }
    }
    fn list(&self) -> vfs::Result<Vec<String>> {
        assert_eq!(self.disk_inode.type_, FileType::Dir);

        Ok((0..self.disk_inode.blocks)
            .map(|i| {
                use vfs::INode;
                let mut entry: DiskEntry = unsafe { uninitialized() };
                self._read_at(i as usize * BLKSIZE, entry.as_buf_mut()).unwrap();
                String::from(entry.name.as_ref())
            }).collect())
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

#[derive(Debug, Eq, PartialEq)]
struct BlockRange {
    block: BlockId,
    begin: usize,
    end: usize,
}

impl BlockRange {
    fn len(&self) -> usize {
        self.end - self.begin
    }
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
///
/// ## 内部可变性
/// 为了方便协调外部及INode对SFS的访问，并为日后并行化做准备，
/// 将SFS设置为内部可变，即对外接口全部是&self，struct的全部field用RefCell包起来
/// 这样其内部各field均可独立访问
pub struct SimpleFileSystem {
    /// on-disk superblock
    super_block: RefCell<Dirty<SuperBlock>>,
    /// blocks in use are mared 0
    free_map: RefCell<Dirty<BitSet>>,
    /// inode list
    inodes: RefCell<BTreeMap<INodeId, Ptr<INode>>>,
    /// device
    device: RefCell<Box<Device>>,
    /// Pointer to self, used by INodes
    self_ptr: Weak<SimpleFileSystem>,
}

impl SimpleFileSystem {
    /// Load SFS from device
    pub fn open(mut device: Box<Device>) -> Option<Rc<Self>> {
        let super_block = device.load_struct::<SuperBlock>(BLKN_SUPER);
        if super_block.check() == false {
            return None;
        }
        let free_map = device.load_struct::<[u8; BLKSIZE]>(BLKN_FREEMAP);

        Some(SimpleFileSystem {
            super_block: RefCell::new(Dirty::new(super_block)),
            free_map: RefCell::new(Dirty::new(BitSet::from_bytes(&free_map))),
            inodes: RefCell::new(BTreeMap::<INodeId, Ptr<INode>>::new()),
            device: RefCell::new(device),
            self_ptr: Weak::default(),
        }.wrap())
    }
    /// Create a new SFS on blank disk
    pub fn create(mut device: Box<Device>, space: usize) -> Rc<Self> {
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
            for i in 3..blocks {
                bitset.insert(i);
            }
            bitset
        };

        let sfs = SimpleFileSystem {
            super_block: RefCell::new(Dirty::new_dirty(super_block)),
            free_map: RefCell::new(Dirty::new_dirty(free_map)),
            inodes: RefCell::new(BTreeMap::<INodeId, Ptr<INode>>::new()),
            device: RefCell::new(device),
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
        sfs.inodes.borrow_mut().insert(BLKN_ROOT, inode);

        sfs
    }
    /// Wrap pure SimpleFileSystem with Rc
    /// Used in constructors
    fn wrap(self) -> Rc<Self> {
        // Create a Rc, make a Weak from it, then put it into the struct.
        // It's a little tricky.
        let mut fs = Rc::new(self);
        // Force create a reference to make Weak
        let fs1 = unsafe { &*(&fs as *const Rc<SimpleFileSystem>) };
        {
            // `Rc::get_mut` is allowed when there is only one strong reference
            // So the following 2 lines can not be joint.
            let fs0 = Rc::get_mut(&mut fs).unwrap();
            fs0.self_ptr = Rc::downgrade(&fs1);
        }
        fs
    }

    /// Allocate a block, return block id
    fn alloc_block(&self) -> Option<usize> {
        let id = self.free_map.borrow_mut().alloc();
        if id.is_some() {
            self.super_block.borrow_mut().unused_blocks -= 1;    // will panic if underflow
        }
        id
    }
    /// Free a block
    fn free_block(&self, block_id: usize) {
        let mut free_map = self.free_map.borrow_mut();
        assert!(!free_map.contains(block_id));
        free_map.insert(block_id);
        self.super_block.borrow_mut().unused_blocks += 1;
    }

    /// Create a new INode struct, then insert it to self.inodes
    /// Private used for load or create INode
    fn _new_inode(&self, id: INodeId, disk_inode: Dirty<DiskINode>) -> Ptr<INode> {
        let inode = Rc::new(RefCell::new(INode {
            disk_inode,
            id,
            fs: self.self_ptr.clone(),
        }));
        self.inodes.borrow_mut().insert(id, inode.clone());
        inode
    }
    /// Get inode by id. Load if not in memory.
    /// ** Must ensure it's a valid INode **
    fn get_inode(&self, id: INodeId) -> Ptr<INode> {
        assert!(!self.free_map.borrow().contains(id));

        // Load if not in memory.
        if !self.inodes.borrow().contains_key(&id) {
            let disk_inode = Dirty::new(self.device.borrow_mut().load_struct::<DiskINode>(id));
            self._new_inode(id, disk_inode)
        } else {
            self.inodes.borrow_mut().get(&id).unwrap().clone()
        }
    }
    /// Create a new INode file
    fn new_inode_file(&self) -> vfs::Result<Ptr<INode>> {
        let id = self.alloc_block().unwrap();
        let disk_inode = Dirty::new_dirty(DiskINode::new_file());
        Ok(self._new_inode(id, disk_inode))
    }
    /// Create a new INode dir
    fn new_inode_dir(&self, parent: INodeId) -> vfs::Result<Ptr<INode>> {
        let id = self.alloc_block().unwrap();
        let disk_inode = Dirty::new_dirty(DiskINode::new_dir());
        let inode = self._new_inode(id, disk_inode);
        inode.borrow_mut().init_dir(parent).unwrap();
        Ok(inode)
    }
}

impl vfs::FileSystem for SimpleFileSystem {
    /// Write back super block if dirty
    fn sync(&self) -> vfs::Result<()> {
        {
            let mut super_block = self.super_block.borrow_mut();
            if super_block.dirty() {
                self.device.borrow_mut().write_at(BLKSIZE * BLKN_SUPER, super_block.as_buf()).unwrap();
                super_block.sync();
            }
        }
        {
            let mut free_map = self.free_map.borrow_mut();
            if free_map.dirty() {
                self.device.borrow_mut().write_at(BLKSIZE * BLKN_FREEMAP, free_map.as_buf()).unwrap();
                free_map.sync();
            }
        }
        for inode in self.inodes.borrow().values() {
            use vfs::INode;
            inode.borrow_mut().sync().unwrap();
        }
        Ok(())
    }

    fn root_inode(&self) -> Ptr<vfs::INode> {
        self.get_inode(BLKN_ROOT)
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
        unsafe { slice::from_raw_parts(slice as *const _ as *const u8, slice.len() * 4) }
    }
    fn as_buf_mut(&mut self) -> &mut [u8] {
        let slice = self.get_ref().storage();
        unsafe { slice::from_raw_parts_mut(slice as *const _ as *mut u8, slice.len() * 4) }
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

    #[test]
    fn block_iter() {
        let mut iter = BlockIter { begin: 0x123, end: 0x2018 };
        assert_eq!(iter.next(), Some(BlockRange { block: 0, begin: 0x123, end: 0x1000 }));
        assert_eq!(iter.next(), Some(BlockRange { block: 1, begin: 0, end: 0x1000 }));
        assert_eq!(iter.next(), Some(BlockRange { block: 2, begin: 0, end: 0x18 }));
        assert_eq!(iter.next(), None);
    }
}