use spin::Mutex;
use alloc::Vec;
//use core::fmt::{Debug, Formatter, Error};

/// On-disk superblock
#[repr(C, packed)]
struct SuperBlock {
    /// magic number, should be SFS_MAGIC
    magic: u32,
    /// number of blocks in fs
    blocks: u32,
    /// number of unused blocks in fs
    unused_blocks: u32,
    /// infomation for sfs
    info: [u8; MAX_INFO_LEN + 1],
}

/// inode (on disk)
#[repr(C, packed)]
#[derive(Debug)]
struct DiskInode {
    /// size of the file (in bytes)
    size: u32,
    /// one of SYS_TYPE_* above
    type_: u16,
    /// number of hard links to this file
    nlinks: u16,
    /// number of blocks
    blocks: u32,
    /// direct blocks
    direct: [u32; NDIRECT],
    /// indirect blocks
    indirect: u32,
    /// double indirect blocks
    db_indirect: u32,
}

/// file entry (on disk)
#[repr(C, packed)]
struct DiskEntry {
    /// inode number
    inode_number: u32,
    /// file name
    name: [u8; MAX_FNAME_LEN + 1],
}

/// inode for sfs
struct Inode {
    /// on-disk inode
    disk_inode: *mut DiskInode,
    /// inode number
    id: u32,
    /// true if inode modified
    dirty: bool,
    /// kill inode if it hits zero
    reclaim_count: u32,
    /// semaphore for din
    mutex: Mutex<()>,
}

/// filesystem for sfs
struct SimpleFileSystem {
    /// on-disk superblock
    super_block: SuperBlock,
    /// blocks in use are mared 0
    free_map: [u8; 0],
    /// true if super/freemap modified
    super_dirty: bool,
    /// buffer for non-block aligned io
    buffer: [u8; 0],
    /// semaphore for fs
    fs_mutex: Mutex<()>,
    /// semaphore for io
    io_mutex: Mutex<()>,
    /// semaphore for link/unlink and rename
    link_mutex: Mutex<()>,
    /// inode list
    inodes: Vec<DiskInode>
}

/*
 * Simple FS (SFS) definitions visible to ucore. This covers the on-disk format
 * and is used by tools that work on SFS volumes, such as mksfs.
 */
/// magic number for sfs
const MAGIC: usize = 0x2f8dbe2a;
/// size of block
const BLKSIZE: usize = 4096;
/// number of direct blocks in inode
const NDIRECT: usize = 12;
/// max length of infomation
const MAX_INFO_LEN: usize = 31;
/// max length of filename
const MAX_FNAME_LEN: usize = 255;
/// max file size (128M)
const MAX_FILE_SIZE: usize = 1024 * 1024 * 128;
/// block the superblock lives in
const BLKN_SUPER: usize = 0;
/// location of the root dir inode
const BLKN_ROOT: usize = 1;
/// 1st block of the freemap
const BLKN_FREEMAP: usize = 2;
/// number of bits in a block
const BLKBITS: usize = BLKSIZE * 8;
/// number of entries in a block
const BLK_NENTRY: usize = BLKSIZE / 4;

/// file types
enum FileType {
    Invalid = 0, File = 1, Dir = 2, Link = 3,
}