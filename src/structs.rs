/// On-disk superblock
#[repr(C, packed)]
pub struct SuperBlock {
    /// magic number, should be SFS_MAGIC
    pub magic: u32,
    /// number of blocks in fs
    pub blocks: u32,
    /// number of unused blocks in fs
    pub unused_blocks: u32,
    /// infomation for sfs
    pub info: [u8; MAX_INFO_LEN + 1],
}

/// inode (on disk)
#[repr(C, packed)]
#[derive(Debug)]
pub struct DiskInode {
    /// size of the file (in bytes)
    pub size: u32,
    /// one of SYS_TYPE_* above
    pub type_: u16,
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

/// file entry (on disk)
#[repr(C, packed)]
pub struct DiskEntry {
    /// inode number
    pub inode_number: u32,
    /// file name
    pub name: [u8; MAX_FNAME_LEN + 1],
}

impl SuperBlock {
    pub fn check(&self) -> bool {
        self.magic == MAGIC
    }
}

/*
 * Simple FS (SFS) definitions visible to ucore. This covers the on-disk format
 * and is used by tools that work on SFS volumes, such as mksfs.
 */
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
pub const BLKN_SUPER: usize = 0;
/// location of the root dir inode
pub const BLKN_ROOT: usize = 1;
/// 1st block of the freemap
pub const BLKN_FREEMAP: usize = 2;
/// number of bits in a block
pub const BLKBITS: usize = BLKSIZE * 8;
/// number of entries in a block
pub const BLK_NENTRY: usize = BLKSIZE / 4;

/// file types
pub enum FileType {
    Invalid = 0, File = 1, Dir = 2, Link = 3,
}