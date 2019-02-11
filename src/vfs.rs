use alloc::{vec::Vec, string::String, sync::Arc};
use core::any::Any;
use core::result;
pub use super::blocked_device::BlockedDevice;

/// Interface for FS to read & write
///     TODO: use std::io::{Read, Write}
pub trait Device: Send {
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;
}

/// Abstract operations on a inode.
pub trait INode: Any + Sync + Send {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn info(&self) -> Result<FileInfo>;
    fn sync(&self) -> Result<()>;
    fn resize(&self, len: usize) -> Result<()>;
    fn create(&self, name: &str, type_: FileType) -> Result<Arc<INode>>;
    fn unlink(&self, name: &str) -> Result<()>;
    /// user of the vfs api should call borrow_mut by itself
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<()>;
    // when self==target use rename instead since it's not possible to have two mut_ref at the same time.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>;
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<Arc<INode>>;
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self, id: usize) -> Result<String>;
    //    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Arc<FileSystem>;
    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any;
}

impl INode {
    pub fn downcast_ref<T: INode>(&self) -> Option<&T> {
        self.as_any_ref().downcast_ref::<T>()
    }
    pub fn list(&self) -> Result<Vec<String>> {
        let info = self.info()?;
        if info.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        (0..info.size).map(|i| {
            self.get_entry(i)
        }).collect()
    }
    pub fn lookup(&self, path: &str) -> Result<Arc<INode>> {
        if self.info()?.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        let mut result = self.find(".")?;
        let mut rest_path = path;
        while rest_path != "" {
            if result.info()?.type_!= FileType::Dir {
                return Err(FsError::NotDir);
            }
            let name;
            match rest_path.find('/') {
                None => {
                    name = rest_path;
                    rest_path = ""
                }
                Some(pos) => {
                    name = &rest_path[0..pos];
                    rest_path = &rest_path[pos + 1..]
                }
            };
            match result.find(name) {
                Err(error) => return Err(error),
                Ok(inode) => result = inode,
            };
        }
        Ok(result)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FileInfo {
    /// Inode number
    pub inode: usize,
    /// Size in bytes
    ///
    /// SFS Note: for normal file size is the actuate file size
    /// for directory this is count of dirent.
    pub size: usize,
    /// Size in blocks
    pub blocks: usize,
    /// Time of last access
    pub atime: Timespec,
    /// Time of last modification
    pub mtime: Timespec,
    /// Time of last change
    pub ctime: Timespec,
    /// Type of file
    pub type_: FileType,
    /// Permission
    pub mode: u16,
    /// Number of hard links
    ///
    /// SFS Note: different from linux, "." and ".." count in nlinks
    /// this is same as original ucore.
    pub nlinks: usize,
    /// User id
    pub uid: usize,
    /// Group id
    pub gid: usize,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Timespec {
    pub sec: i64,
    pub nsec: i32
}

#[derive(Debug, Eq, PartialEq)]
pub enum FileType {
    File,
    Dir,
}

#[derive(Debug)]
pub struct FsInfo {
    pub max_file_size: usize,
}

// Note: IOError/NoMemory always lead to a panic since it's hard to recover from it.
//       We also panic when we can not parse the fs on disk normally
#[derive(Debug)]
pub enum FsError {
    NotSupported,//E_UNIMP, or E_INVAL
    NotFile,//E_ISDIR
    IsDir,//E_ISDIR, used only in link
    NotDir,//E_NOTDIR
    EntryNotFound,//E_NOENT
    EntryExist,//E_EXIST
    NotSameFs,//E_XDEV
    InvalidParam,//E_INVAL
    NoDeviceSpace,//E_NOSPC, but is defined and not used in the original ucore, which uses E_NO_MEM
    DirRemoved,//E_NOENT, when the current dir was remove by a previous unlink
    DirNotEmpty,//E_NOTEMPTY
    WrongFs,//E_INVAL, when we find the content on disk is wrong when opening the device
}

pub type Result<T> = result::Result<T,FsError>;

/// Abstract filesystem
pub trait FileSystem: Sync {
    fn sync(&self) -> Result<()>;
    fn root_inode(&self) -> Arc<INode>;
    fn info(&self) -> &'static FsInfo;
//    fn unmount(&self) -> Result<()>;
//    fn cleanup(&self);
}
