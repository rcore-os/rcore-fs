#![cfg_attr(not(any(test, feature = "std")), no_std)]

extern crate alloc;
extern crate log;

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    sync::{Arc, Weak},
};
use core::any::Any;
use rcore_fs::vfs::*;
use spin::RwLock;

pub mod special;

/// Device file system
///
/// The filesystem for all device files.
/// It should be mounted at /dev.
///
/// The file system is readonly from the root INode.
/// You can add or remove devices through `add()` and `remove()`.
pub struct DevFS {
    devs: RwLock<BTreeMap<String, Arc<dyn INode>>>,
    self_ref: Weak<DevFS>,
}

impl FileSystem for DevFS {
    fn sync(&self) -> Result<()> {
        Ok(())
    }

    fn root_inode(&self) -> Arc<dyn INode> {
        Arc::new(DevRootINode {
            fs: self.self_ref.upgrade().unwrap(),
        })
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

impl DevFS {
    pub fn new() -> Arc<Self> {
        DevFS {
            devs: RwLock::new(BTreeMap::new()),
            self_ref: Weak::default(),
        }
        .wrap()
    }
    pub fn add(&self, name: &str, dev: Arc<dyn INode>) -> Result<()> {
        let mut devs = self.devs.write();
        if devs.contains_key(name) {
            return Err(FsError::EntryExist);
        }
        devs.insert(String::from(name), dev);
        Ok(())
    }
    pub fn remove(&self, name: &str) -> Result<()> {
        let mut devs = self.devs.write();
        devs.remove(name).ok_or(FsError::EntryNotFound)?;
        Ok(())
    }
    /// Wrap pure DevFS with Arc
    /// Used in constructors
    fn wrap(self) -> Arc<Self> {
        // Create an Arc, make a Weak from it, then put it into the struct.
        // It's a little tricky.
        let fs = Arc::new(self);
        let weak = Arc::downgrade(&fs);
        let ptr = Arc::into_raw(fs) as *mut Self;
        unsafe {
            (*ptr).self_ref = weak;
        }
        unsafe { Arc::from_raw(ptr) }
    }
}

struct DevRootINode {
    /// Reference to FS
    fs: Arc<DevFS>,
}

impl INode for DevRootINode {
    fn read_at(&self, _offset: usize, _buf: &mut [u8]) -> Result<usize> {
        Err(FsError::IsDir)
    }

    fn write_at(&self, _offset: usize, _buf: &[u8]) -> Result<usize> {
        Err(FsError::IsDir)
    }

    fn poll(&self) -> Result<PollStatus> {
        Err(FsError::IsDir)
    }

    fn metadata(&self) -> Result<Metadata> {
        Ok(Metadata {
            dev: 0,
            inode: 1,
            size: self.fs.devs.read().len(),
            blk_size: 0,
            blocks: 0,
            atime: Timespec { sec: 0, nsec: 0 },
            mtime: Timespec { sec: 0, nsec: 0 },
            ctime: Timespec { sec: 0, nsec: 0 },
            type_: FileType::Dir,
            mode: 0o666,
            nlinks: 2,
            uid: 0,
            gid: 0,
            rdev: 0,
        })
    }

    fn set_metadata(&self, _metadata: &Metadata) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn sync_all(&self) -> Result<()> {
        Ok(())
    }

    fn sync_data(&self) -> Result<()> {
        Ok(())
    }

    fn resize(&self, _len: usize) -> Result<()> {
        Err(FsError::IsDir)
    }

    fn create(&self, _name: &str, _type_: FileType, _mode: u32) -> Result<Arc<dyn INode>> {
        Err(FsError::NotSupported)
    }

    fn link(&self, _name: &str, _other: &Arc<dyn INode>) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _name: &str) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn move_(&self, _old_name: &str, _target: &Arc<dyn INode>, _new_name: &str) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn find(&self, name: &str) -> Result<Arc<dyn INode>> {
        match name {
            "." | ".." => Ok(self.fs.root_inode()),
            name => self
                .fs
                .devs
                .read()
                .get(name)
                .map(|d| d.clone())
                .ok_or(FsError::EntryNotFound),
        }
    }

    fn get_entry(&self, id: usize) -> Result<(usize, FileType, String)> {
        match id {
            0 => Ok((
                self.metadata().unwrap().inode,
                FileType::Dir,
                String::from("."),
            )),
            1 => Ok((
                self.metadata().unwrap().inode,
                FileType::Dir,
                String::from(".."),
            )),
            i => {
                if let Some((name, inode)) = self.fs.devs.read().iter().nth(i - 2) {
                    let metadata = inode.metadata()?;
                    Ok((metadata.inode, metadata.type_, name.to_string()))
                } else {
                    Err(FsError::EntryNotFound)
                }
            }
        }
    }

    fn io_control(&self, _cmd: u32, _data: usize) -> Result<usize> {
        Err(FsError::NotSupported)
    }

    fn mmap(&self, _area: MMapArea) -> Result<()> {
        Err(FsError::NotSupported)
    }

    fn fs(&self) -> Arc<dyn FileSystem> {
        self.fs.clone()
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}
