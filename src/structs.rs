//! On-disk structures in SFS

use core::slice;
use core::mem::size_of_val;
use core::fmt::{Debug, Formatter, Error};

/// On-disk superblock
#[repr(C, packed)]
#[derive(Debug)]
pub struct SuperBlock {
    /// magic number, should be SFS_MAGIC
    pub magic: u32,
    /// number of blocks in fs
    pub blocks: u32,
    /// number of unused blocks in fs
    pub unused_blocks: u32,
    /// information for sfs
    pub info: Str32,
}

/// inode (on disk)
#[repr(C, packed)]
#[derive(Debug)]
pub struct DiskINode {
    /// size of the file (in bytes)
    pub size: u32,
    /// one of SYS_TYPE_* above
    pub type_: FileType,
    /// number of hard links to this file
    pub nlinks: u16,
    /// number of blocks
    pub blocks: u32,
    /// direct blocks
    pub direct: [u32; NDIRECT],
    /// indirect blocks
    pub indirect: u32,
    /// double indirect blocks
    pub db_indirect: u32,
}

#[repr(C, packed)]
pub struct IndirectBlock {
    pub entries: [u32; BLK_NENTRY],
}

/// file entry (on disk)
#[repr(C, packed)]
#[derive(Debug)]
pub struct DiskEntry {
    /// inode number
    pub id: u32,
    /// file name
    pub name: Str256,
}

#[repr(C)]
pub struct Str256(pub [u8; 256]);
#[repr(C)]
pub struct Str32(pub [u8; 32]);

impl Debug for Str256 {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        use alloc::String;
        let len = self.0.iter().enumerate().find(|(i, &b)| b == 0).unwrap().0;
        write!(f, "{}", String::from_utf8_lossy(&self.0[0 .. len]))
    }
}
impl Debug for Str32 {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        use alloc::String;
        let len = self.0.iter().enumerate().find(|(i, &b)| b == 0).unwrap().0;
        write!(f, "{}", String::from_utf8_lossy(&self.0[0 .. len]))
    }
}
impl Str32 {
    pub fn from_slice(s: &[u8]) -> Self {
        let mut ret = [0u8; 32];
        ret[0..s.len()].copy_from_slice(s);
        Str32(ret)
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
        unsafe{ slice::from_raw_parts(self as *const _ as *const u8, size_of_val(self)) }
    }
    fn as_buf_mut(&mut self) -> &mut [u8] {
        unsafe{ slice::from_raw_parts_mut(self as *mut _ as *mut u8, size_of_val(self)) }
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
pub const BLKSIZE: usize = 4096;
/// number of direct blocks in inode
pub const NDIRECT: usize = 12;
/// max length of infomation
pub const MAX_INFO_LEN: usize = 31;
/// max length of filename
pub const MAX_FNAME_LEN: usize = 255;
/// max file size (128M)
pub const MAX_FILE_SIZE: usize = 1024 * 1024 * 128;
/// block the superblock lives in
pub const BLKN_SUPER: BlockId = 0;
/// location of the root dir inode
pub const BLKN_ROOT: BlockId = 1;
/// 1st block of the freemap
pub const BLKN_FREEMAP: BlockId = 2;
/// number of bits in a block
pub const BLKBITS: usize = BLKSIZE * 8;
///
pub const ENTRY_SIZE: usize = 4;
/// number of entries in a block
pub const BLK_NENTRY: usize = BLKSIZE / ENTRY_SIZE;

/// file types
#[repr(u16)]
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum FileType {
    Invalid = 0, File = 1, Dir = 2, Link = 3,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn struct_size() {
        use core::mem::size_of;
        assert!(size_of::<SuperBlock>() <= BLKSIZE);
        assert!(size_of::<DiskINode>() <= BLKSIZE);
        assert!(size_of::<DiskEntry>() <= BLKSIZE);
        assert_eq!(size_of::<IndirectBlock>(), BLKSIZE);
    }
}