//! On-disk structures in SEFS

use alloc::str;
use core::fmt::{Debug, Error, Formatter};
use core::mem::{size_of, size_of_val};
use core::slice;
use static_assertions::const_assert;

use super::dev::{SefsMac, SefsUuid};

/// On-disk superblock
#[repr(C)]
#[derive(Debug)]
pub struct SuperBlock {
    /// magic number, should be SFS_MAGIC
    pub magic: u32,
    /// number of blocks in fs
    pub blocks: u32,
    /// number of unused blocks in fs
    pub unused_blocks: u32,
    /// number of block groups
    pub groups: u32,
}

/// On-disk inode
#[repr(C)]
#[derive(Debug)]
pub struct DiskINode {
    /// size of the file (in bytes)
    pub size: u32,
    /// one of SYS_TYPE_* above
    pub type_: FileType,
    /// permission
    pub mode: u16,
    /// number of hard links to this file
    /// Note: "." and ".." is counted in this nlinks
    pub nlinks: u16,
    /// number of blocks
    pub blocks: u32,
    pub uid: u16,
    pub gid: u8,
    pub atime: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub disk_filename: SefsUuid,
    pub inode_mac: SefsMac,
}

/// On-disk file entry
#[repr(C)]
#[derive(Debug)]
pub struct DiskEntry {
    /// inode number
    pub id: u32,
    /// file name
    pub name: Str256,
}

#[repr(C)]
pub struct Str256(pub [u8; 256]);

impl AsRef<str> for Str256 {
    fn as_ref(&self) -> &str {
        let len = self.0.iter().enumerate().find(|(_, &b)| b == 0).unwrap().0;
        str::from_utf8(&self.0[0..len]).unwrap()
    }
}

impl Debug for Str256 {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.as_ref())
    }
}

impl<'a> From<&'a str> for Str256 {
    fn from(s: &'a str) -> Self {
        let mut ret = [0u8; 256];
        ret[0..s.len()].copy_from_slice(s.as_ref());
        Str256(ret)
    }
}

impl SuperBlock {
    pub fn check(&self) -> bool {
        self.magic == MAGIC
    }
}

/// Convert structs to [u8] slice
pub trait AsBuf {
    fn as_buf(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const _ as *const u8, size_of_val(self)) }
    }
    fn as_buf_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self as *mut _ as *mut u8, size_of_val(self)) }
    }
}

impl AsBuf for SuperBlock {}

impl AsBuf for DiskINode {}

impl AsBuf for DiskEntry {}

impl AsBuf for u32 {}

/*
 * Simple FS (SFS) definitions visible to ucore. This covers the on-disk format
 * and is used by tools that work on SFS volumes, such as mksfs.
 */
pub type BlockId = usize;
pub type INodeId = BlockId;

/// magic number for sfs
pub const MAGIC: u32 = 0x2f8dbe2a;
/// size of block
pub const BLKSIZE: usize = 1usize << BLKSIZE_LOG2;
/// log2( size of block )
pub const BLKSIZE_LOG2: u8 = 7;
/// max length of filename
pub const MAX_FNAME_LEN: usize = 255;
/// block the superblock lives in
pub const BLKN_SUPER: BlockId = 0;
/// location of the root dir inode
pub const BLKN_ROOT: BlockId = 2;
/// 1st block of the freemap
pub const BLKN_FREEMAP: BlockId = 1;
/// number of bits in a block
pub const BLKBITS: usize = BLKSIZE * 8;
/// size of a dirent used in the size field
pub const DIRENT_SIZE: usize = 260;

pub const METAFILE_NAME: &str = "metadata";

/// file types
#[repr(u16)]
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum FileType {
    Invalid = 0,
    File = 1,
    Dir = 2,
    SymLink = 3,
}

const_assert!(o1; size_of::<SuperBlock>() <= BLKSIZE);
const_assert!(o2; size_of::<DiskINode>() <= BLKSIZE);
