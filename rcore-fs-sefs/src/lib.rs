#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]

#[macro_use]
extern crate alloc;
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    prelude::ToString,
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::any::Any;
use core::fmt::{Debug, Error, Formatter};
use core::mem::uninitialized;

use bitvec::BitVec;
use rcore_fs::dev::TimeProvider;
use rcore_fs::dirty::Dirty;
use rcore_fs::vfs::{self, FileSystem, FsError, INode, Timespec};
use spin::RwLock;

use self::dev::*;
use self::structs::*;

pub mod dev;
mod structs;

/// Helper methods for `File`
impl dyn File {
    fn read_block(&self, id: BlockId, buf: &mut [u8]) -> DevResult<()> {
        assert!(buf.len() <= BLKSIZE);
        self.read_exact_at(buf, id * BLKSIZE)
    }
    fn write_block(&self, id: BlockId, buf: &[u8]) -> DevResult<()> {
        assert!(buf.len() <= BLKSIZE);
        self.write_all_at(buf, id * BLKSIZE)
    }
    fn read_direntry(&self, id: usize) -> DevResult<DiskEntry> {
        let mut direntry: DiskEntry = unsafe { uninitialized() };
        self.read_exact_at(direntry.as_buf_mut(), DIRENT_SIZE * id)?;
        Ok(direntry)
    }
    fn write_direntry(&self, id: usize, direntry: &DiskEntry) -> DevResult<()> {
        self.write_all_at(direntry.as_buf(), DIRENT_SIZE * id)
    }
    /// Load struct `T` from given block in device
    fn load_struct<T: AsBuf>(&self, id: BlockId) -> DevResult<T> {
        let mut s: T = unsafe { uninitialized() };
        self.read_block(id, s.as_buf_mut())?;
        Ok(s)
    }
}

/// inode for SEFS
pub struct INodeImpl {
    /// inode number
    id: INodeId,
    /// on-disk inode
    disk_inode: RwLock<Dirty<DiskINode>>,
    /// back file
    file: Box<dyn File>,
    /// Reference to FS
    fs: Arc<SEFS>,
}

impl Debug for INodeImpl {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "INode {{ id: {}, disk: {:?} }}",
            self.id, self.disk_inode
        )
    }
}

impl INodeImpl {
    /// Only for Dir
    fn get_file_inode_and_entry_id(&self, name: &str) -> Option<(INodeId, usize)> {
        (0..self.disk_inode.read().blocks as usize)
            .map(|i| {
                let entry = self.file.read_direntry(i).unwrap();
                (entry, i)
            })
            .find(|(entry, _)| entry.name.as_ref() == name)
            .map(|(entry, id)| (entry.id as INodeId, id))
    }
    fn get_file_inode_id(&self, name: &str) -> Option<INodeId> {
        self.get_file_inode_and_entry_id(name)
            .map(|(inode_id, _)| inode_id)
    }
    /// Init dir content. Insert 2 init entries.
    /// This do not init nlinks, please modify the nlinks in the invoker.
    fn dirent_init(&self, parent: INodeId) -> vfs::Result<()> {
        self.disk_inode.write().blocks = 2;
        // Insert entries: '.' '..'
        self.file.write_direntry(
            0,
            &DiskEntry {
                id: self.id as u32,
                name: Str256::from("."),
            },
        )?;
        self.file.write_direntry(
            1,
            &DiskEntry {
                id: parent as u32,
                name: Str256::from(".."),
            },
        )?;
        Ok(())
    }
    fn dirent_append(&self, entry: &DiskEntry) -> vfs::Result<()> {
        let mut inode = self.disk_inode.write();
        let total = &mut inode.blocks;
        self.file.write_direntry(*total as usize, entry)?;
        *total += 1;
        Ok(())
    }
    /// remove a page in middle of file and insert the last page here, useful for dirent remove
    /// should be only used in unlink
    fn dirent_remove(&self, id: usize) -> vfs::Result<()> {
        let total = self.disk_inode.read().blocks as usize;
        debug_assert!(id < total);
        let last_direntry = self.file.read_direntry(total - 1)?;
        if id != total - 1 {
            self.file.write_direntry(id, &last_direntry)?;
        }
        self.file.set_len((total - 1) * DIRENT_SIZE)?;
        self.disk_inode.write().blocks -= 1;
        Ok(())
    }
    fn nlinks_inc(&self) {
        self.disk_inode.write().nlinks += 1;
    }
    fn nlinks_dec(&self) {
        let mut disk_inode = self.disk_inode.write();
        assert!(disk_inode.nlinks > 0);
        disk_inode.nlinks -= 1;
    }
    #[cfg(feature = "create_image")]
    pub fn update_mac(&self) -> vfs::Result<()> {
        if self.fs.device.is_integrity_only() {
            self.disk_inode.write().inode_mac = self.file.get_file_mac().unwrap();
            //println!("file_mac {:?}", self.disk_inode.read().inode_mac);
            self.sync_all()?;
        }
        Ok(())
    }
    #[cfg(not(feature = "create_image"))]
    fn check_integrity(&self) {
        if self.fs.device.is_integrity_only() {
            let inode_mac = &self.disk_inode.read().inode_mac;
            let file_mac = self.file.get_file_mac().unwrap();
            //info!("inode_mac {:?}, file_mac {:?}", inode_mac, file_mac);
            let not_integrity = inode_mac.0 != file_mac.0;
            assert!(!not_integrity, "FsError::NoIntegrity");
        }
    }
}

impl vfs::INode for INodeImpl {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> vfs::Result<usize> {
        let type_ = self.disk_inode.read().type_;
        if type_ != FileType::File && type_ != FileType::SymLink {
            return Err(FsError::NotFile);
        }
        let len = self.file.read_at(buf, offset)?;
        Ok(len)
    }
    fn write_at(&self, offset: usize, buf: &[u8]) -> vfs::Result<usize> {
        let DiskINode { type_, size, .. } = **self.disk_inode.read();
        if type_ != FileType::File && type_ != FileType::SymLink {
            return Err(FsError::NotFile);
        }
        let end_offset = offset + buf.len();
        if (size as usize) < end_offset {
            self.resize(end_offset)?;
        }
        let len = self.file.write_at(buf, offset)?;
        Ok(len)
    }
    fn poll(&self) -> vfs::Result<vfs::PollStatus> {
        Ok(vfs::PollStatus {
            read: true,
            write: true,
            error: false,
        })
    }
    /// the size returned here is logical size(entry num for directory), not the disk space used.
    fn metadata(&self) -> vfs::Result<vfs::Metadata> {
        let disk_inode = self.disk_inode.read();
        Ok(vfs::Metadata {
            dev: 0,
            inode: self.id,
            size: match disk_inode.type_ {
                FileType::File | FileType::SymLink => disk_inode.size as usize,
                FileType::Dir => disk_inode.blocks as usize,
                _ => panic!("Unknown file type"),
            },
            mode: disk_inode.mode,
            type_: vfs::FileType::from(disk_inode.type_.clone()),
            blocks: disk_inode.blocks as usize,
            atime: Timespec {
                sec: disk_inode.atime as i64,
                nsec: 0,
            },
            mtime: Timespec {
                sec: disk_inode.mtime as i64,
                nsec: 0,
            },
            ctime: Timespec {
                sec: disk_inode.ctime as i64,
                nsec: 0,
            },
            nlinks: disk_inode.nlinks as usize,
            uid: disk_inode.uid as usize,
            gid: disk_inode.gid as usize,
            blk_size: 0x1000,
            rdev: 0,
        })
    }
    fn set_metadata(&self, metadata: &vfs::Metadata) -> vfs::Result<()> {
        let mut disk_inode = self.disk_inode.write();
        disk_inode.mode = metadata.mode;
        disk_inode.uid = metadata.uid as u16;
        disk_inode.gid = metadata.gid as u8;
        disk_inode.atime = metadata.atime.sec as u32;
        disk_inode.mtime = metadata.mtime.sec as u32;
        disk_inode.ctime = metadata.ctime.sec as u32;
        Ok(())
    }
    fn sync_all(&self) -> vfs::Result<()> {
        let mut disk_inode = self.disk_inode.write();
        if disk_inode.dirty() {
            self.fs
                .meta_file
                .write_block(self.id, disk_inode.as_buf())?;
            disk_inode.sync();
        }
        self.sync_data()?;
        Ok(())
    }
    fn sync_data(&self) -> vfs::Result<()> {
        self.file.flush()?;
        Ok(())
    }
    fn resize(&self, len: usize) -> vfs::Result<()> {
        let type_ = self.disk_inode.read().type_;
        if type_ != FileType::File && type_ != FileType::SymLink {
            return Err(FsError::NotFile);
        }
        self.file.set_len(len)?;
        self.disk_inode.write().size = len as u32;
        Ok(())
    }
    fn create(
        &self,
        name: &str,
        type_: vfs::FileType,
        mode: u32,
    ) -> vfs::Result<Arc<dyn vfs::INode>> {
        let type_ = match type_ {
            vfs::FileType::File => FileType::File,
            vfs::FileType::Dir => FileType::Dir,
            vfs::FileType::SymLink => FileType::SymLink,
            _ => return Err(vfs::FsError::InvalidParam),
        };
        let info = self.metadata()?;
        if info.type_ != vfs::FileType::Dir {
            return Err(FsError::NotDir);
        }
        if info.nlinks <= 0 {
            return Err(FsError::DirRemoved);
        }

        // Ensure the name is not exist
        if !self.get_file_inode_id(name).is_none() {
            return Err(FsError::EntryExist);
        }

        // Create new INode
        let inode = self.fs.new_inode(type_, mode as u16)?;
        if type_ == FileType::Dir {
            inode.dirent_init(self.id)?;
        }

        // Write new entry
        let entry = DiskEntry {
            id: inode.id as u32,
            name: Str256::from(name),
        };
        self.dirent_append(&entry)?;
        inode.nlinks_inc();
        if type_ == FileType::Dir {
            inode.nlinks_inc(); //for .
            self.nlinks_inc(); //for ..
        }

        Ok(inode)
    }
    fn unlink(&self, name: &str) -> vfs::Result<()> {
        let info = self.metadata()?;
        if info.type_ != vfs::FileType::Dir {
            return Err(FsError::NotDir);
        }
        if info.nlinks <= 0 {
            return Err(FsError::DirRemoved);
        }
        if name == "." {
            return Err(FsError::IsDir);
        }
        if name == ".." {
            return Err(FsError::IsDir);
        }

        let (inode_id, entry_id) = self
            .get_file_inode_and_entry_id(name)
            .ok_or(FsError::EntryNotFound)?;
        let inode = self.fs.get_inode(inode_id);

        let type_ = inode.disk_inode.read().type_;
        if type_ == FileType::Dir {
            // only . and ..
            assert!(inode.disk_inode.read().blocks >= 2);
            if inode.disk_inode.read().blocks > 2 {
                return Err(FsError::DirNotEmpty);
            }
        }
        inode.nlinks_dec();
        if type_ == FileType::Dir {
            inode.nlinks_dec(); //for .
            self.nlinks_dec(); //for ..
        }
        self.dirent_remove(entry_id)?;

        Ok(())
    }
    fn link(&self, name: &str, other: &Arc<dyn INode>) -> vfs::Result<()> {
        let info = self.metadata()?;
        if info.type_ != vfs::FileType::Dir {
            return Err(FsError::NotDir);
        }
        if info.nlinks <= 0 {
            return Err(FsError::DirRemoved);
        }
        if !self.get_file_inode_id(name).is_none() {
            return Err(FsError::EntryExist);
        }
        let child = other
            .downcast_ref::<INodeImpl>()
            .ok_or(FsError::NotSameFs)?;
        if !Arc::ptr_eq(&self.fs, &child.fs) {
            return Err(FsError::NotSameFs);
        }
        if child.metadata()?.type_ == vfs::FileType::Dir {
            return Err(FsError::IsDir);
        }
        let entry = DiskEntry {
            id: child.id as u32,
            name: Str256::from(name),
        };
        self.dirent_append(&entry)?;
        child.nlinks_inc();
        Ok(())
    }
    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> vfs::Result<()> {
        let info = self.metadata()?;
        if info.type_ != vfs::FileType::Dir {
            return Err(FsError::NotDir);
        }
        if info.nlinks <= 0 {
            return Err(FsError::DirRemoved);
        }
        if old_name == "." {
            return Err(FsError::IsDir);
        }
        if old_name == ".." {
            return Err(FsError::IsDir);
        }

        let dest = target
            .downcast_ref::<INodeImpl>()
            .ok_or(FsError::NotSameFs)?;
        let dest_info = dest.metadata()?;
        if !Arc::ptr_eq(&self.fs, &dest.fs) {
            return Err(FsError::NotSameFs);
        }
        if dest_info.type_ != vfs::FileType::Dir {
            return Err(FsError::NotDir);
        }
        if dest_info.nlinks <= 0 {
            return Err(FsError::DirRemoved);
        }
        if dest.get_file_inode_id(new_name).is_some() {
            return Err(FsError::EntryExist);
        }

        let (inode_id, entry_id) = self
            .get_file_inode_and_entry_id(old_name)
            .ok_or(FsError::EntryNotFound)?;
        if info.inode == dest_info.inode {
            // rename: in place modify name
            let entry = DiskEntry {
                id: inode_id as u32,
                name: Str256::from(new_name),
            };
            self.file.write_direntry(entry_id, &entry)?;
        } else {
            // move
            let inode = self.fs.get_inode(inode_id);

            let entry = DiskEntry {
                id: inode_id as u32,
                name: Str256::from(new_name),
            };
            dest.dirent_append(&entry)?;
            self.dirent_remove(entry_id)?;

            if inode.metadata()?.type_ == vfs::FileType::Dir {
                self.nlinks_dec();
                dest.nlinks_inc();
            }
        }

        Ok(())
    }
    fn find(&self, name: &str) -> vfs::Result<Arc<dyn vfs::INode>> {
        let info = self.metadata()?;
        if info.type_ != vfs::FileType::Dir {
            return Err(FsError::NotDir);
        }
        let inode_id = self.get_file_inode_id(name).ok_or(FsError::EntryNotFound)?;
        Ok(self.fs.get_inode(inode_id))
    }
    fn get_entry(&self, id: usize) -> vfs::Result<String> {
        if self.disk_inode.read().type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        if id >= self.disk_inode.read().blocks as usize {
            return Err(FsError::EntryNotFound);
        };
        let entry = self.file.read_direntry(id)?;
        Ok(String::from(entry.name.as_ref()))
    }
    fn io_control(&self, _cmd: u32, _data: usize) -> vfs::Result<()> {
        Err(FsError::NotSupported)
    }
    fn fs(&self) -> Arc<dyn vfs::FileSystem> {
        self.fs.clone()
    }
    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
impl Drop for INodeImpl {
    /// Auto sync when drop
    fn drop(&mut self) {
        #[cfg(feature = "create_image")]
        self.update_mac()
            .expect("failed to update mac when dropping the SEFS Inode");

        self.sync_all()
            .expect("Failed to sync when dropping the SEFS Inode");
        if self.disk_inode.read().nlinks <= 0 {
            self.disk_inode.write().sync();
            self.fs.free_block(self.id);
            let disk_filename = &self.disk_inode.read().disk_filename;
            let filename = disk_filename.to_string();
            self.fs.device.remove(filename.as_str()).unwrap();
        }
    }
}

/// Simple Encrypted File System
pub struct SEFS {
    /// on-disk superblock
    super_block: RwLock<Dirty<SuperBlock>>,
    /// blocks in use are marked 0
    free_map: RwLock<Dirty<BitVec>>,
    /// inode list
    inodes: RwLock<BTreeMap<INodeId, Weak<INodeImpl>>>,
    /// device
    device: Box<dyn Storage>,
    /// metadata file
    meta_file: Box<dyn File>,
    /// Time provider
    time_provider: &'static dyn TimeProvider,
    /// uuid provider
    uuid_provider: &'static dyn UuidProvider,
    /// Pointer to self, used by INodes
    self_ptr: Weak<SEFS>,
}

impl SEFS {
    /// Load SEFS
    pub fn open(
        device: Box<dyn Storage>,
        time_provider: &'static dyn TimeProvider,
        uuid_provider: &'static dyn UuidProvider,
    ) -> vfs::Result<Arc<Self>> {
        let meta_file = device.open(METAFILE_NAME)?;
        let super_block = meta_file.load_struct::<SuperBlock>(BLKN_SUPER)?;
        if !super_block.check() {
            return Err(FsError::WrongFs);
        }

        // load free map
        let mut free_map = BitVec::with_capacity(BLKBITS * super_block.groups as usize);
        unsafe {
            free_map.set_len(BLKBITS * super_block.groups as usize);
        }
        for i in 0..super_block.groups as usize {
            let block_id = Self::get_freemap_block_id_of_group(i);
            meta_file.read_block(
                block_id,
                &mut free_map.as_mut()[BLKSIZE * i..BLKSIZE * (i + 1)],
            )?;
        }

        Ok(SEFS {
            super_block: RwLock::new(Dirty::new(super_block)),
            free_map: RwLock::new(Dirty::new(free_map)),
            inodes: RwLock::new(BTreeMap::new()),
            device,
            meta_file,
            time_provider,
            uuid_provider,
            self_ptr: Weak::default(),
        }
        .wrap())
    }
    /// Create a new SEFS
    pub fn create(
        device: Box<dyn Storage>,
        time_provider: &'static dyn TimeProvider,
        uuid_provider: &'static dyn UuidProvider,
    ) -> vfs::Result<Arc<Self>> {
        let blocks = BLKBITS;

        let super_block = SuperBlock {
            magic: MAGIC,
            blocks: blocks as u32,
            unused_blocks: blocks as u32 - 2,
            groups: 1,
        };
        let free_map = {
            let mut bitset = BitVec::with_capacity(BLKBITS);
            bitset.extend(core::iter::repeat(false).take(BLKBITS));
            for i in 2..blocks {
                bitset.set(i, true);
            }
            bitset
        };
        let meta_file = device.create(METAFILE_NAME)?;
        meta_file.set_len(blocks * BLKSIZE)?;

        let mode = match device.is_integrity_only() {
            true => 0o444,
            false => 0o644,
        };

        let sefs = SEFS {
            super_block: RwLock::new(Dirty::new_dirty(super_block)),
            free_map: RwLock::new(Dirty::new_dirty(free_map)),
            inodes: RwLock::new(BTreeMap::new()),
            device,
            meta_file,
            time_provider,
            uuid_provider,
            self_ptr: Weak::default(),
        }
        .wrap();
        // Init root INode
        let root = sefs.new_inode(FileType::Dir, mode)?;
        assert_eq!(root.id, BLKN_ROOT);
        root.dirent_init(BLKN_ROOT)?;
        root.nlinks_inc(); //for .
        root.nlinks_inc(); //for ..(root's parent is itself)
        root.sync_all()?;

        Ok(sefs)
    }
    /// Wrap pure SEFS with Arc
    /// Used in constructors
    fn wrap(self) -> Arc<Self> {
        // Create a Arc, make a Weak from it, then put it into the struct.
        // It's a little tricky.
        let fs = Arc::new(self);
        let weak = Arc::downgrade(&fs);
        let ptr = Arc::into_raw(fs) as *mut Self;
        unsafe {
            (*ptr).self_ptr = weak;
        }
        unsafe { Arc::from_raw(ptr) }
    }

    /// Allocate a block, return block id
    fn alloc_block(&self) -> Option<usize> {
        let mut free_map = self.free_map.write();
        let mut super_block = self.super_block.write();
        let id = free_map.alloc().or_else(|| {
            // allocate a new group
            let new_group_id = super_block.groups as usize;
            super_block.groups += 1;
            super_block.blocks += BLKBITS as u32;
            super_block.unused_blocks += BLKBITS as u32 - 1;
            self.meta_file
                .set_len(super_block.groups as usize * BLKBITS * BLKSIZE)
                .expect("failed to extend meta file");
            free_map.extend(core::iter::repeat(true).take(BLKBITS));
            free_map.set(Self::get_freemap_block_id_of_group(new_group_id), false);
            // allocate block again
            free_map.alloc()
        });
        assert!(id.is_some(), "allocate block should always success");
        super_block.unused_blocks -= 1;
        id
    }
    /// Free a block
    fn free_block(&self, block_id: usize) {
        let mut free_map = self.free_map.write();
        assert!(!free_map[block_id]);
        free_map.set(block_id, true);
        self.super_block.write().unused_blocks += 1;
    }

    /// Create a new INode struct, then insert it to self.inodes
    /// Private used for load or create INode
    fn _new_inode(
        &self,
        id: INodeId,
        disk_inode: Dirty<DiskINode>,
        create: bool,
    ) -> Arc<INodeImpl> {
        let filename = disk_inode.disk_filename.to_string();

        let inode = Arc::new(INodeImpl {
            id,
            disk_inode: RwLock::new(disk_inode),
            file: match create {
                true => self.device.create(filename.as_str()).unwrap(),
                false => self.device.open(filename.as_str()).unwrap(),
            },
            fs: self.self_ptr.upgrade().unwrap(),
        });
        #[cfg(not(feature = "create_image"))]
        match create {
            false => inode.check_integrity(),
            _ => {},
        };
        self.inodes.write().insert(id, Arc::downgrade(&inode));
        inode
    }
    /// Get inode by id. Load if not in memory.
    /// ** Must ensure it's a valid INode **
    fn get_inode(&self, id: INodeId) -> Arc<INodeImpl> {
        assert!(!self.free_map.read()[id]);

        // In the BTreeSet and not weak.
        if let Some(inode) = self.inodes.read().get(&id) {
            if let Some(inode) = inode.upgrade() {
                return inode;
            }
        }
        // Load if not in set, or is weak ref.
        let disk_inode = Dirty::new(self.meta_file.load_struct::<DiskINode>(id).unwrap());
        self._new_inode(id, disk_inode, false)
    }

    /// Create a new INode file
    fn new_inode(&self, type_: FileType, mode: u16) -> vfs::Result<Arc<INodeImpl>> {
        let id = self.alloc_block().ok_or(FsError::NoDeviceSpace)?;
        let time = self.time_provider.current_time().sec as u32;
        let uuid = self.uuid_provider.generate_uuid();
        let disk_inode = Dirty::new_dirty(DiskINode {
            size: 0,
            type_,
            mode,
            nlinks: 0,
            blocks: 0,
            uid: 0,
            gid: 0,
            atime: time,
            mtime: time,
            ctime: time,
            disk_filename: uuid,
            inode_mac: Default::default(),
        });
        Ok(self._new_inode(id, disk_inode, true))
    }
    fn flush_weak_inodes(&self) {
        let mut inodes = self.inodes.write();
        let remove_ids: Vec<_> = inodes
            .iter()
            .filter(|(_, inode)| inode.upgrade().is_none())
            .map(|(&id, _)| id)
            .collect();
        for id in remove_ids.iter() {
            inodes.remove(&id);
        }
    }
    fn get_freemap_block_id_of_group(group_id: usize) -> usize {
        BLKBITS * group_id + BLKN_FREEMAP
    }
}

impl vfs::FileSystem for SEFS {
    /// Write back super block if dirty
    fn sync(&self) -> vfs::Result<()> {
        // sync super_block
        let mut super_block = self.super_block.write();
        if super_block.dirty() {
            self.meta_file
                .write_all_at(super_block.as_buf(), BLKSIZE * BLKN_SUPER)?;
            super_block.sync();
        }
        // sync free_map
        let mut free_map = self.free_map.write();
        if free_map.dirty() {
            for i in 0..super_block.groups as usize {
                let slice = &free_map.as_ref()[BLKSIZE * i..BLKSIZE * (i + 1)];
                self.meta_file
                    .write_all_at(slice, BLKSIZE * Self::get_freemap_block_id_of_group(i))?;
            }
            free_map.sync();
        }
        // sync all INodes
        self.flush_weak_inodes();
        for inode in self.inodes.read().values() {
            if let Some(inode) = inode.upgrade() {
                inode.sync_all()?;
            }
        }
        self.meta_file.flush()?;
        Ok(())
    }

    fn root_inode(&self) -> Arc<dyn vfs::INode> {
        self.get_inode(BLKN_ROOT)
    }

    fn info(&self) -> vfs::FsInfo {
        let sb = self.super_block.read();
        vfs::FsInfo {
            bsize: BLKSIZE,
            frsize: BLKSIZE,
            blocks: sb.blocks as usize,
            bfree: sb.unused_blocks as usize,
            bavail: sb.unused_blocks as usize,
            files: sb.blocks as usize,        // inaccurate
            ffree: sb.unused_blocks as usize, // inaccurate
            namemax: MAX_FNAME_LEN,
        }
    }
}

impl Drop for SEFS {
    /// Auto sync when drop
    fn drop(&mut self) {
        self.sync().expect("Failed to sync when dropping the SEFS");
    }
}

trait BitsetAlloc {
    fn alloc(&mut self) -> Option<usize>;
}

impl BitsetAlloc for BitVec {
    fn alloc(&mut self) -> Option<usize> {
        // TODO: more efficient
        let id = (0..self.len()).find(|&i| self[i]);
        if let Some(id) = id {
            self.set(id, false);
        }
        id
    }
}

impl AsBuf for BitVec {
    fn as_buf(&self) -> &[u8] {
        self.as_ref()
    }
    fn as_buf_mut(&mut self) -> &mut [u8] {
        self.as_mut()
    }
}

impl AsBuf for [u8; BLKSIZE] {}

impl From<FileType> for vfs::FileType {
    fn from(t: FileType) -> Self {
        match t {
            FileType::File => vfs::FileType::File,
            FileType::Dir => vfs::FileType::Dir,
            FileType::SymLink => vfs::FileType::SymLink,
            _ => panic!("unknown file type"),
        }
    }
}
