#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]

extern crate alloc;
#[macro_use]
extern crate log;

use alloc::{
    string::{String, ToString},
    sync::{Arc, Weak},
    vec,
    vec::Vec,
};
use core::any::Any;
use rcore_fs::vfs::*;
use spin::{RwLock, RwLockWriteGuard};

pub struct RamFS {
    root: Arc<dyn INode>,
}

impl FileSystem for RamFS {
    fn sync(&self) -> Result<()> {
        Ok(())
    }

    fn root_inode(&self) -> Arc<dyn INode> {
        Arc::clone(&self.root)
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

struct RamFSINode {
    parent: Option<Weak<dyn INode>>,
    this: Option<Weak<dyn INode>>,
    children: Vec<(String, Arc<dyn INode>)>,
    content: Vec<u8>,
    extra: Metadata,
    fs: Weak<RamFS>,
}

struct LockedINode(RwLock<RamFSINode>);

impl INode for LockedINode {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        let file = self.0.read();
        if file.extra.type_ == FileType::Dir {
            return Err(FsError::IsDir);
        }
        let start = file.content.len().min(offset);
        let end = file.content.len().min(offset + buf.len());
        let src = &file.content[start..end];
        buf[0..src.len()].copy_from_slice(src);
        Ok(src.len())
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        let mut file = self.0.write();
        if file.extra.type_ == FileType::Dir {
            return Err(FsError::IsDir);
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
        if file.extra.type_ == FileType::Dir {
            return Err(FsError::IsDir);
        }
        Ok(PollStatus {
            read: true,
            write: true,
            error: false,
        })
    }

    fn metadata(&self) -> Result<Metadata> {
        let file = self.0.read();
        let extra = &file.extra;
        let size = file.content.len();
        Ok(Metadata {
            dev: 0,
            inode: extra.inode,
            size: size,
            blk_size: 4096,
            blocks: size / 4096,
            atime: extra.atime,
            mtime: extra.mtime,
            ctime: extra.ctime,
            type_: extra.type_,
            mode: extra.mode,
            nlinks: extra.nlinks,
            uid: extra.uid,
            gid: extra.gid,
            rdev: extra.rdev,
        })
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
        if file.extra.type_ == FileType::File {
            file.content.resize(len, 0);
            Ok(())
        } else {
            Err(FsError::NotFile)
        }
    }

    fn create2(
        &self,
        name: &str,
        type_: FileType,
        mode: u32,
        data: usize,
    ) -> Result<Arc<dyn INode>> {
        let mut file = self.0.write();
        if file.extra.type_ == FileType::Dir {
            if name == "." {
                return Err(FsError::EntryExist);
            }
            if name == ".." {
                return Err(FsError::EntryExist);
            }
            for (n, _) in file.children.iter() {
                if n == name {
                    return Err(FsError::EntryExist);
                }
            }
            let temp_file: Arc<dyn INode> = Arc::new(LockedINode(RwLock::new(RamFSINode {
                parent: Some(Weak::clone(file.this.as_ref().unwrap())),
                this: None,
                children: Vec::new(),
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
                    type_: type_,
                    mode: mode as u16,
                    nlinks: 0,
                    uid: 0,
                    gid: 0,
                    rdev: data,
                },
                fs: Weak::clone(&file.fs),
            })));
            let mut root = downcast_inode(Arc::as_ref(&temp_file)).0.write();
            root.this = Some(Arc::downgrade(&temp_file));
            drop(root);
            file.children
                .push((String::from(name), Arc::clone(&temp_file)));
            Ok(temp_file)
        } else {
            Err(FsError::NotDir)
        }
    }

    fn link(&self, name: &str, other: &Arc<dyn INode>) -> Result<()> {
        let other_lock = downcast_inode(Arc::as_ref(other));
        // to make sure locking order.
        let mut locks = lockMultiple(&vec![&self.0, &other_lock.0]).into_iter();

        let mut file = locks.next().unwrap();
        let mut other_l = locks.next().unwrap();

        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        if other_l.extra.type_ == FileType::Dir {
            return Err(FsError::IsDir);
        }
        for (n, _) in file.children.iter() {
            if n == name {
                return Err(FsError::EntryExist);
            }
        }

        file.children.push((String::from(name), Arc::clone(other)));
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
        let mut index: usize = 0;
        for (n, f) in file.children.iter() {
            if n == name {
                let removal_inode = Arc::clone(f);
                let other = downcast_inode(Arc::as_ref(&removal_inode));
                if other.0.read().children.len() > 0 {
                    return Err(FsError::DirNotEmpty);
                }
                file.children.remove(index);
                drop(file);
                other.0.write().extra.nlinks -= 1;
                return Ok(());
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> Result<()> {
        let elem = self.find(old_name)?;
        let t = downcast_inode(Arc::as_ref(target));
        if let Err(err) = t.link(new_name, &elem) {
            return Err(err);
        } else {
            if let Err(err) = self.unlink(old_name) {
                t.unlink(new_name)?;
                return Err(err);
            } else {
                return Ok(());
            }
        }
    }

    fn find(&self, name: &str) -> Result<Arc<dyn INode>> {
        let file = self.0.read();
        if file.extra.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        //info!("find it: {} {}", name, file.parent.is_none());
        match name {
            "." => Ok(file
                .this
                .as_ref()
                .unwrap()
                .upgrade()
                .ok_or(FsError::EntryNotFound)?),
            ".." => Ok(file
                .parent
                .as_ref()
                .unwrap()
                .upgrade()
                .ok_or(FsError::EntryNotFound)?),
            name => {
                for (s, e) in file.children.iter() {
                    if s == name {
                        return Ok(Arc::clone(e));
                    }
                }
                Err(FsError::EntryNotFound)
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
                if let Some((s, _)) = file.children.get(i - 2) {
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

fn lockMultiple<'a>(locks: &[&'a RwLock<RamFSINode>]) -> Vec<RwLockWriteGuard<'a, RamFSINode>> {
    let mut v: Vec<(usize, &'a RwLock<RamFSINode>, usize)> = Vec::new();
    let mut index: usize = 0;
    for l in locks {
        v.push((index, *l, l.read().extra.inode));
        index += 1;
    }
    v.sort_by_key(|l| l.2);
    let mut v2: Vec<(usize, RwLockWriteGuard<'a, RamFSINode>)> = v
        .into_iter()
        .map(|(index, lock, _inode)| (index, lock.write()))
        .collect();
    v2.sort_by_key(|(index, _lock)| *index);
    v2.into_iter().map(|(_index, lock)| lock).collect()
}

fn downcast_inode<'a>(inode: &'a dyn INode) -> &'a LockedINode {
    inode.as_any_ref().downcast_ref::<LockedINode>().unwrap()
}
