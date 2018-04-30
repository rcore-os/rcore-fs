//! C Interfaces for ucore

/// ﻿Abstract low-level file.
///
/// Match struct `inode` in ucore `kern/fs/vfs/inode.h`
#[repr(C)]
struct INode {
    // TODO: full the struct
}

/// ﻿A buffer Rd/Wr status record
///
/// Match struct `iobuf` in ucore `kern/fs/iobuf.h`
#[repr(C)]
struct IoBuf {
    /// the base addr of buffer (used for Rd/Wr)
    base: *mut u8,
    /// current Rd/Wr position in buffer, will have been incremented by the amount transferred
    offset: i32,
    /// the length of buffer  (used for Rd/Wr)
    len: u32,
    /// current resident length need to Rd/Wr, will have been decremented by the amount transferred.
    resident: u32,
}

/// Information about a file
///
/// Match struct `stat` in ucore `libs/stat.h`
#[repr(C)]
struct Stat {
    /// protection mode and file type
    mode: u32,
    /// number of hard links
    nlinks: u32,
    /// number of blocks file is using
    blocks: u32,
    /// file size (bytes)
    size: u32,
}

const S_IFMT : u32 = 070000;          // mask for type of file
const S_IFREG: u32 = 010000;          // ordinary regular file
const S_IFDIR: u32 = 020000;          // directory
const S_IFLNK: u32 = 030000;          // symbolic link
const S_IFCHR: u32 = 040000;          // character device
const S_IFBLK: u32 = 050000;          // block device

/// ﻿Abstract operations on a inode.
///
/// Match struct `inode_ops` in ucore `kern/fs/vfs/inode.h`
// TODO: Append docs from ucore
#[repr(C)]
struct INodeOps {
    magic: u64,
    open: extern fn(*mut INode, flags: u32) -> i32,
    close: extern fn(*mut INode) -> i32,
    read: extern fn(*mut INode, *mut IoBuf) -> i32,
    write: extern fn(*mut INode, *mut IoBuf) -> i32,
    fstat: extern fn(*mut INode, *mut Stat) -> i32,
    fsync: extern fn(*mut INode) -> i32,
    namefile: extern fn(*mut INode, *mut IoBuf) -> i32,
    getdirentry: extern fn(*mut INode, *mut IoBuf) -> i32,
    reclaim: extern fn(*mut INode) -> i32,
    gettype: extern fn(*mut INode, type_store: *mut u32) -> i32,
    tryseek: extern fn(*mut INode, pos: i32) -> i32,
    truncate: extern fn(*mut INode, len: i32) -> i32,
    create: extern fn(*mut INode, name: *const u8, excl: bool, inode_store: *mut *mut INode) -> i32,
    lookup: extern fn(*mut INode, path: *mut u8, inode_store: *mut *mut INode) -> i32,
    ioctl: extern fn(*mut INode, op: i32, data: *mut u8) -> i32,
}