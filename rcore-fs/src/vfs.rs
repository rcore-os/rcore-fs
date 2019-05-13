use crate::dev::DevError;
use alloc::{string::String, sync::Arc, vec::Vec};
use core::any::Any;
use core::fmt;
use core::result;
use core::str;

/// Abstract file system object such as file or directory.
pub trait INode: Any + Sync + Send {
    /// Read bytes at `offset` into `buf`, return the number of bytes read.
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;

    /// Write bytes at `offset` from `buf`, return the number of bytes written.
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;

    /// Poll the events, return a bitmap of events.
    fn poll(&self) -> Result<PollStatus>;

    /// Get metadata of the INode
    fn metadata(&self) -> Result<Metadata>;

    /// Set metadata of the INode
    fn set_metadata(&self, metadata: &Metadata) -> Result<()>;

    /// Sync all data and metadata
    fn sync_all(&self) -> Result<()>;

    /// Sync data (not include metadata)
    fn sync_data(&self) -> Result<()>;

    /// Resize the file
    fn resize(&self, len: usize) -> Result<()>;

    /// Create a new INode in the directory
    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<INode>>;

    /// Create a hard link `name` to `other`
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>;

    /// Delete a hard link `name`
    fn unlink(&self, name: &str) -> Result<()>;

    /// Move INode `self/old_name` to `target/new_name`.
    /// If `target` equals `self`, do rename.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>;

    /// Find the INode `name` in the directory
    fn find(&self, name: &str) -> Result<Arc<INode>>;

    /// Get the name of directory entry
    fn get_entry(&self, id: usize) -> Result<String>;

    /// Control device
    fn io_control(&self, cmd: u32, data: usize) -> Result<()>;

    /// Get the file system of the INode
    fn fs(&self) -> Arc<FileSystem>;

    /// This is used to implement dynamics cast.
    /// Simply return self in the implement of the function.
    fn as_any_ref(&self) -> &Any;
}

impl INode {
    /// Downcast the INode to specific struct
    pub fn downcast_ref<T: INode>(&self) -> Option<&T> {
        self.as_any_ref().downcast_ref::<T>()
    }

    /// Get all directory entries as a Vec
    pub fn list(&self) -> Result<Vec<String>> {
        let info = self.metadata()?;
        if info.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }
        Ok((0..)
            .map(|i| self.get_entry(i))
            .take_while(|result| result.is_ok())
            .filter_map(|result| result.ok())
            .collect())
    }

    /// Lookup path from current INode, and do not follow symlinks
    pub fn lookup(&self, path: &str) -> Result<Arc<INode>> {
        self.lookup_follow(path, 0)
    }

    /// Lookup path from current INode, and follow symlinks at most `follow_times` times
    pub fn lookup_follow(&self, path: &str, mut follow_times: usize) -> Result<Arc<INode>> {
        if self.metadata()?.type_ != FileType::Dir {
            return Err(FsError::NotDir);
        }

        let mut result = self.find(".")?;
        let mut rest_path = String::from(path);
        while rest_path != "" {
            if result.metadata()?.type_ != FileType::Dir {
                return Err(FsError::NotDir);
            }
            // handle absolute path
            if let Some('/') = rest_path.chars().next() {
                result = self.fs().root_inode();
                rest_path = String::from(&rest_path[1..]);
                continue;
            }
            let name;
            match rest_path.find('/') {
                None => {
                    name = rest_path;
                    rest_path = String::new();
                }
                Some(pos) => {
                    name = String::from(&rest_path[0..pos]);
                    rest_path = String::from(&rest_path[pos + 1..]);
                }
            };
            let inode = result.find(&name)?;
            // Handle symlink
            if inode.metadata()?.type_ == FileType::SymLink && follow_times > 0 {
                follow_times -= 1;
                let mut content = [0u8; 256];
                let len = inode.read_at(0, &mut content)?;
                let path = str::from_utf8(&content[..len]).map_err(|_| FsError::NotDir)?;
                // result remains unchanged
                rest_path = {
                    let mut new_path = String::from(path);
                    if let Some('/') = new_path.chars().last() {
                        new_path += &rest_path;
                    } else {
                        new_path += "/";
                        new_path += &rest_path;
                    }
                    new_path
                };
            } else {
                result = inode
            }
        }
        Ok(result)
    }
}

pub enum IOCTLError {
    NotValidFD = 9,      // EBADF
    NotValidMemory = 14, // EFAULT
    NotValidParam = 22,  // EINVAL
    NotCharDevice = 25,  // ENOTTY
}

#[derive(Debug, Default)]
pub struct PollStatus {
    pub read: bool,
    pub write: bool,
    pub error: bool,
}

/// Metadata of INode
///
/// Ref: [http://pubs.opengroup.org/onlinepubs/009604499/basedefs/sys/stat.h.html]
#[derive(Debug, Eq, PartialEq)]
pub struct Metadata {
    /// Device ID
    pub dev: usize, // (major << 8) | minor
    /// Inode number
    pub inode: usize,
    /// Size in bytes
    ///
    /// SFS Note: for normal file size is the actuate file size
    /// for directory this is count of dirent.
    pub size: usize,
    /// A file system-specific preferred I/O block size for this object.
    /// In some file system types, this may vary from file to file.
    pub blk_size: usize,
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
    /// User ID
    pub uid: usize,
    /// Group ID
    pub gid: usize,
    /// Raw device id
    /// e.g. /dev/null: makedev(0x1, 0x3)
    pub rdev: usize, // (major << 8) | minor
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Timespec {
    pub sec: i64,
    pub nsec: i32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileType {
    File,
    Dir,
    SymLink,
    CharDevice,
    BlockDevice,
    NamedPipe,
    Socket,
}

/// Metadata of FileSystem
///
/// Ref: [http://pubs.opengroup.org/onlinepubs/9699919799/]
#[derive(Debug)]
pub struct FsInfo {
    /// File system block size
    pub bsize: usize,
    /// Fundamental file system block size
    pub frsize: usize,
    /// Total number of blocks on file system in units of `frsize`
    pub blocks: usize,
    /// Total number of free blocks
    pub bfree: usize,
    /// Number of free blocks available to non-privileged process
    pub bavail: usize,
    /// Total number of file serial numbers
    pub files: usize,
    /// Total number of free file serial numbers
    pub ffree: usize,
    /// Maximum filename length
    pub namemax: usize,
}

// Note: IOError/NoMemory always lead to a panic since it's hard to recover from it.
//       We also panic when we can not parse the fs on disk normally
#[derive(Debug)]
pub enum FsError {
    NotSupported,  //E_UNIMP, or E_INVAL
    NotFile,       //E_ISDIR
    IsDir,         //E_ISDIR, used only in link
    NotDir,        //E_NOTDIR
    EntryNotFound, //E_NOENT
    EntryExist,    //E_EXIST
    NotSameFs,     //E_XDEV
    InvalidParam,  //E_INVAL
    NoDeviceSpace, //E_NOSPC, but is defined and not used in the original ucore, which uses E_NO_MEM
    DirRemoved,    //E_NOENT, when the current dir was remove by a previous unlink
    DirNotEmpty,   //E_NOTEMPTY
    WrongFs,       //E_INVAL, when we find the content on disk is wrong when opening the device
    DeviceError,
    IOCTLError,
    NoDevice,
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<DevError> for FsError {
    fn from(_: DevError) -> Self {
        FsError::DeviceError
    }
}

#[cfg(any(test, feature = "std"))]
impl std::error::Error for FsError {}

pub type Result<T> = result::Result<T, FsError>;

/// Abstract file system
pub trait FileSystem: Sync {
    /// Sync all data to the storage
    fn sync(&self) -> Result<()>;

    /// Get the root INode of the file system
    fn root_inode(&self) -> Arc<INode>;

    /// Get the file system information
    fn info(&self) -> FsInfo;
}
