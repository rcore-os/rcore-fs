#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]

extern crate alloc;
#[macro_use]
extern crate log;

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::{Arc, Weak},
    vec::Vec,
};
use core::any::Any;
use core::sync::atomic::*;
use rcore_fs::vfs::*;
use spin::{RwLock, RwLockWriteGuard};

pub struct RamFS {
    root: Arc<LockedINode>,
    next_inode_id: AtomicUsize,
}

impl FileSystem for RamFS {
    fn sync(&self) -> Result<()> {
        Ok(())
    }

    fn root_inode(&self) -> Arc<dyn INode> {
        Arc::clone(&self.root) as _
    }

    fn info(&self) -> FsInfo {
        FsInfo {
            bsize: 0,
            frsize: 0,
            blocks: 0,
            bfree: 0,
            bavail: 0,
            files: 0,
            ffree: 0,
            namemax: 0,
        }
    }
}

impl RamFS {
    pub fn new() -> Arc<Self> {
        let root = Arc::new(LockedINode(RwLock::new(RamFSINode {
            this: Weak::default(),
            parent: Weak::default(),
            children: BTreeMap::new(),
            content: Vec::new(),
            extra: Metadata {
                dev: 0,
                inode: 0,
                size: 0,
                blk_size: 0,
                blocks: 0,
                atime: Timespec { sec: 0, nsec: 0 },
                mtime: Timespec { sec: 0, nsec: 0 },
                ctime: Timespec { sec: 0, nsec: 0 },
                type_: FileType::Dir,
                mode: 0o777,
                nlinks: 1,
                uid: 0,
                gid: 0,
                rdev: 0,
            },
            fs: Weak::default(),
        })));
        let fs = Arc::new(RamFS {
            root,
            next_inode_id: AtomicUsize::new(1),
        });
        let mut root = fs.root.0.write();
        root.parent = Arc::downgrade(&fs.root);
        root.this = Arc::downgrade(&fs.root);
        root.fs = Arc::downgrade(&fs);
        root.extra.inode =
            Arc::into_raw(root.this.upgrade().unwrap()) as *const RamFSINode as usize;
        drop(root);
        fs
    }

    /// Allocate an INode ID
    fn alloc_inode_id(&self) -> usize {
        self.next_inode_id.fetch_add(1, Ordering::SeqCst)
    }
}

struct RamFSINode {
    /// Reference to parent INode
    parent: Weak<LockedINode>,
    /// Reference to myself
    this: Weak<LockedINode>,
    /// Reference to children INodes
    children: BTreeMap<String, Arc<LockedINode>>,
    /// Content of the file
    content: Vec<u8>,
    /// INode metadata
    extra: Metadata,
    /// Reference to FS
    fs: Weak<RamFS>,
}

struct LockedINode(RwLock<RamFSINode>);

impl INode for LockedINode {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        let file = self.0.read();
        if file.extra.type_ != FileType::File {
            return Err(FsError::NotFile);
        }
        let start = file.content.len().min(offset);
        let end = file.content.len().min(offset + buf.len());
        let src = &file.content[start..end];
        buf[0..src.len()].copy_from_slice(src);
        Ok(src.len())
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        let mut file = self.0.write();
        if file.extra.type_ != FileType::File {
            return Err(FsError::NotFile);
        }
        let content = &mut file.content;
        if offset + buf.len() > content.len() {
            content.resize(offset + buf.len(), 0);
        }
        let target = &mut content[offset..offset + buf.len()];
        target.copy_from_slice(buf);
        Ok(buf.len())
    }

    fn poll(&self) -> Result<PollStatus> {
        let file = self.0.read();
        if file.extra.type_ != FileType::File {
            return Err(FsError::NotFile);
        }
        Ok(PollStatus {
            read: true,
            write: true,
            error: false,
        })
    }

    fn metadata(&self) -> Result<Metadata> {
        let file = self.0.read();
        let mut metadata = file.extra.clone();
        metadata.size = file.content.len();
        Ok(metadata)
    }

    fn set_metadata(&self, metadata: &Metadata) -> Result<()> {
        let mut file = self.0.write();
        file.extra.atime = metadata.atime;
        file.extra.mtime = metadata.mtime;
        file.extra.ctime = metadata.ctime;
        file.extra.mode = metadata.mode;
        file.extra.uid = metadata.uid;
        file.extra.gid = metadata.gid;
        Ok(())
    }

    fn sync_all(&self) -> Result<()> {
        Ok(())
    }

    fn sync_data(&self) -> Result<()> {
        Ok(())
    }

    fn resize(&self, len: usize) -> Result<()> {
        let mut file = self.0.write();
        if file.extra.type_ != FileType::File {
            return Err(FsError::NotFile);
        }
        file.content.resize(len, 0);
        Ok(())
    }

    fn create2(
        &self,
        name: &str,
        type_: FileType,
        mode: u32,
        data: usize,
    ) -> Result<Arc<dyn INode>> {
        let mut file = self.0.write();
        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        if name == "." || name == ".." {
            return Err(FsError::EntryExist);
        }
        if file.children.contains_key(name) {
            return Err(FsError::EntryExist);
        }
        let temp_file = Arc::new(LockedINode(RwLock::new(RamFSINode {
            parent: Weak::clone(&file.this),
            this: Weak::default(),
            children: BTreeMap::new(),
            content: Vec::new(),
            extra: Metadata {
                dev: 0,
                inode: file.fs.upgrade().unwrap().alloc_inode_id(),
                size: 0,
                blk_size: 0,
                blocks: 0,
                atime: Timespec { sec: 0, nsec: 0 },
                mtime: Timespec { sec: 0, nsec: 0 },
                ctime: Timespec { sec: 0, nsec: 0 },
                type_,
                mode: mode as u16,
                nlinks: 1,
                uid: 0,
                gid: 0,
                rdev: data,
            },
            fs: Weak::clone(&file.fs),
        })));
        temp_file.0.write().this = Arc::downgrade(&temp_file);
        file.children
            .insert(String::from(name), Arc::clone(&temp_file));
        Ok(temp_file)
    }

    fn link(&self, name: &str, other: &Arc<dyn INode>) -> Result<()> {
        let other = other
            .downcast_ref::<LockedINode>()
            .ok_or(FsError::NotSameFs)?;
        // to make sure locking order.
        let mut locks = lock_multiple(&[&self.0, &other.0]).into_iter();

        let mut file = locks.next().unwrap();
        let mut other_l = locks.next().unwrap();

        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        if other_l.extra.type_ == FileType::Dir {
            return Err(FsError::IsDir);
        }
        if file.children.contains_key(name) {
            return Err(FsError::EntryExist);
        }

        file.children
            .insert(String::from(name), other_l.this.upgrade().unwrap());
        other_l.extra.nlinks += 1;
        Ok(())
    }

    fn unlink(&self, name: &str) -> Result<()> {
        let mut file = self.0.write();
        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        if name == "." || name == ".." {
            return Err(FsError::DirNotEmpty);
        }
        let other = file.children.get(name).ok_or(FsError::EntryNotFound)?;
        if other.0.read().children.len() > 0 {
            return Err(FsError::DirNotEmpty);
        }
        other.0.write().extra.nlinks -= 1;
        file.children.remove(name);
        Ok(())
    }

    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> Result<()> {
        let elem = self.find(old_name)?;
        target.link(new_name, &elem)?;
        if let Err(err) = self.unlink(old_name) {
            // recover
            target.unlink(new_name)?;
            return Err(err);
        }
        Ok(())
    }

    fn find(&self, name: &str) -> Result<Arc<dyn INode>> {
        let file = self.0.read();
        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        //info!("find it: {} {}", name, file.parent.is_none());
        match name {
            "." => Ok(file.this.upgrade().ok_or(FsError::EntryNotFound)?),
            ".." => Ok(file.parent.upgrade().ok_or(FsError::EntryNotFound)?),
            name => {
                let s = file.children.get(name).ok_or(FsError::EntryNotFound)?;
                Ok(Arc::clone(s) as Arc<dyn INode>)
            }
        }
    }

    fn get_entry(&self, id: usize) -> Result<String> {
        let file = self.0.read();
        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }

        match id {
            0 => Ok(String::from(".")),
            1 => Ok(String::from("..")),
            i => {
                if let Some(s) = file.children.keys().nth(i - 2) {
                    Ok(s.to_string())
                } else {
                    Err(FsError::EntryNotFound)
                }
            }
        }
    }

    fn io_control(&self, _cmd: u32, _data: usize) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn fs(&self) -> Arc<dyn FileSystem> {
        Weak::upgrade(&self.0.read().fs).unwrap()
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}

/// Lock INodes order by their inode id
fn lock_multiple<'a>(locks: &[&'a RwLock<RamFSINode>]) -> Vec<RwLockWriteGuard<'a, RamFSINode>> {
    let mut order: Vec<usize> = (0..locks.len()).collect();
    order.sort_by_key(|&i| locks[i].read().extra.inode);
    order.iter().map(|&i| locks[i].write()).collect()
}
