#![feature(get_mut_unchecked)]

use core::any::Any;
use rcore_fs::vfs::*;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::string::String;
use std::sync::{Arc, Weak};
use std::sync::{Mutex, MutexGuard};

#[macro_use]
extern crate log;

/// File system at host
pub struct HostFS {
    path: PathBuf,
    self_ref: Weak<HostFS>,
}

/// INode for `HostFS`
pub struct HNode {
    path: PathBuf,
    file: Mutex<Option<std::fs::File>>,
    fs: Arc<HostFS>,
}

impl FileSystem for HostFS {
    fn sync(&self) -> Result<()> {
        warn!("HostFS: sync is unimplemented");
        Ok(())
    }

    fn root_inode(&self) -> Arc<dyn INode> {
        Arc::new(HNode {
            path: self.path.clone(),
            file: Mutex::new(None),
            fs: self.self_ref.upgrade().unwrap(),
        })
    }

    fn info(&self) -> FsInfo {
        let statvfs =
            nix::sys::statvfs::statvfs(&self.path).expect("fail to get info from host fs");
        FsInfo {
            bsize: statvfs.block_size() as _,
            frsize: statvfs.fragment_size() as _,
            blocks: statvfs.blocks() as _,
            bfree: statvfs.blocks_free() as _,
            bavail: statvfs.blocks_available() as _,
            files: statvfs.files() as _,
            ffree: statvfs.files_free() as _,
            namemax: statvfs.name_max() as _,
        }
    }
}

impl HostFS {
    /// Create a new `HostFS` from host `path`
    pub fn new(path: impl AsRef<Path>) -> Arc<HostFS> {
        HostFS {
            path: path.as_ref().to_path_buf(),
            self_ref: Weak::default(),
        }
        .wrap()
    }

    /// Wrap pure `HostFS` with Arc
    /// Used in constructors
    fn wrap(self) -> Arc<Self> {
        // Create an Arc, make a Weak from it, then put it into the struct.
        let mut fs = Arc::new(self);
        unsafe {
            Arc::get_mut_unchecked(&mut fs).self_ref = Arc::downgrade(&fs);
        }
        fs
    }
}

impl INode for HNode {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        let mut guard = self.open_file()?;
        let file = guard.as_mut().unwrap();
        file.seek(SeekFrom::Start(offset as u64))?;
        let len = file.read(buf)?;
        Ok(len)
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        let mut guard = self.open_file()?;
        let file = guard.as_mut().unwrap();
        file.seek(SeekFrom::Start(offset as u64))?;
        let len = file.write(buf)?;
        Ok(len)
    }

    fn poll(&self) -> Result<PollStatus> {
        unimplemented!()
    }

    fn metadata(&self) -> Result<Metadata> {
        let metadata = self.path.metadata()?;
        Ok(metadata.into())
    }

    fn set_metadata(&self, metadata: &Metadata) -> Result<()> {
        // TODO 仅修改了文件的最后访问时间和最后修改时间
        use nix::{
            libc::{timespec, AT_FDCWD},
            sys::{
                stat::{utimensat, UtimensatFlags::FollowSymlink},
                time::TimeSpec,
            },
        };
        utimensat(
            Some(AT_FDCWD),
            &self.path,
            &TimeSpec::from_timespec(timespec {
                tv_sec: metadata.atime.sec,
                tv_nsec: metadata.atime.nsec as _,
            }),
            &TimeSpec::from_timespec(timespec {
                tv_sec: metadata.mtime.sec,
                tv_nsec: metadata.mtime.nsec as _,
            }),
            FollowSymlink,
        )
        .map_err(|_| FsError::InvalidParam)
    }

    fn sync_all(&self) -> Result<()> {
        self.open_file()?.as_mut().unwrap().sync_all()?;
        Ok(())
    }

    fn sync_data(&self) -> Result<()> {
        self.open_file()?.as_mut().unwrap().sync_data()?;
        Ok(())
    }

    fn resize(&self, len: usize) -> Result<()> {
        self.open_file()?.as_mut().unwrap().set_len(len as u64)?;
        Ok(())
    }

    fn create(&self, name: &str, type_: FileType, _mode: u32) -> Result<Arc<dyn INode>> {
        let new_path = self.path.join(name);
        if new_path.exists() {
            return Err(FsError::EntryExist);
        }
        match type_ {
            FileType::File => {
                std::fs::File::create(&new_path)?;
            }
            FileType::Dir => {
                std::fs::create_dir(&new_path)?;
            }
            _ => unimplemented!("only support creating file or dir in HostFS"),
        }
        Ok(Arc::new(HNode {
            path: new_path,
            file: Mutex::new(None),
            fs: self.fs.clone(),
        }))
    }

    fn link(&self, name: &str, other: &Arc<dyn INode>) -> Result<()> {
        let other = other.downcast_ref::<Self>().ok_or(FsError::NotSameFs)?;
        std::fs::hard_link(&other.path, &self.path.join(name))?;
        Ok(())
    }

    fn unlink(&self, name: &str) -> Result<()> {
        let new_path = self.path.join(name);
        if new_path.is_file() {
            std::fs::remove_file(new_path)?;
        } else if new_path.is_dir() {
            std::fs::remove_dir(new_path)?;
        } else {
            return Err(FsError::EntryNotFound);
        }
        Ok(())
    }

    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> Result<()> {
        let target = target.downcast_ref::<Self>().ok_or(FsError::NotSameFs)?;
        let old_path = self.path.join(old_name);
        let new_path = target.path.join(new_name);
        std::fs::rename(old_path, new_path)?;
        Ok(())
    }

    fn find(&self, name: &str) -> Result<Arc<dyn INode>> {
        let new_path = self.path.join(name);
        if new_path.exists() {
            Ok(Arc::new(HNode {
                path: new_path,
                file: Mutex::new(None),
                fs: self.fs.clone(),
            }))
        } else {
            Err(FsError::EntryNotFound)
        }
    }

    fn get_entry(&self, id: usize) -> Result<String> {
        if self.path.is_dir() {
            self.path
                .read_dir()?
                .nth(id)
                .ok_or(FsError::EntryNotFound)??
                .file_name()
                .into_string()
                .map_err(|_| FsError::InvalidParam)
        } else {
            Err(FsError::NotDir)
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

impl HNode {
    /// Ensure to open the file and store a `File` into `self.file`,
    /// return the `MutexGuard`.
    /// If the type of `self.path` is not file, then return Err
    fn open_file(&self) -> Result<MutexGuard<Option<std::fs::File>>> {
        if !self.path.exists() {
            return Err(FsError::EntryNotFound);
        }
        if !self.path.is_file() {
            return Err(FsError::NotFile);
        }
        let mut maybe_file = self.file.lock().unwrap();
        if maybe_file.is_none() {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&self.path)?;
            *maybe_file = Some(file);
        }
        Ok(maybe_file)
    }
}
