//! C Interfaces for ucore

pub use self::allocator::UcoreAllocator;

#[no_mangle]
pub static SFS_INODE_OPS: INodeOps = INodeOps::from_rust_inode::<sfs::INode>();

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

/// mask for type of file
const S_IFMT: u32 = 070000;
/// ordinary regular file
const S_IFREG: u32 = 010000;
/// directory
const S_IFDIR: u32 = 020000;
/// symbolic link
const S_IFLNK: u32 = 030000;
/// character device
const S_IFCHR: u32 = 040000;
/// block device
const S_IFBLK: u32 = 050000;

/// ﻿Abstract operations on a inode.
///
/// Match struct `inode_ops` in ucore `kern/fs/vfs/inode.h`
// TODO: Append docs from ucore
#[repr(C)]
pub struct INodeOps {
    magic: u64,
    open: extern fn(*mut INode, flags: u32) -> ErrorCode,
    close: extern fn(*mut INode) -> ErrorCode,
    read: extern fn(*mut INode, *mut IoBuf) -> ErrorCode,
    write: extern fn(*mut INode, *mut IoBuf) -> ErrorCode,
    fstat: extern fn(*mut INode, *mut Stat) -> ErrorCode,
    fsync: extern fn(*mut INode) -> ErrorCode,
    namefile: extern fn(*mut INode, *mut IoBuf) -> ErrorCode,
    getdirentry: extern fn(*mut INode, *mut IoBuf) -> ErrorCode,
    reclaim: extern fn(*mut INode) -> ErrorCode,
    gettype: extern fn(*mut INode, type_store: *mut u32) -> ErrorCode,
    tryseek: extern fn(*mut INode, pos: i32) -> ErrorCode,
    truncate: extern fn(*mut INode, len: i32) -> ErrorCode,
    create: extern fn(*mut INode, name: *const u8, excl: bool, inode_store: *mut *mut INode) -> ErrorCode,
    lookup: extern fn(*mut INode, path: *mut u8, inode_store: *mut *mut INode) -> ErrorCode,
    ioctl: extern fn(*mut INode, op: i32, data: *mut u8) -> ErrorCode,
}

#[repr(i32)]
#[derive(Debug)]
pub enum ErrorCode {
    Ok = 0,
    Unimplemented = -1,
}

use vfs;
use sfs;

impl INodeOps {
    const fn from_rust_inode<T: vfs::INode>() -> Self {
        extern fn open(inode: *mut INode, flags: u32) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn close(inode: *mut INode) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn read(inode: *mut INode, buf: *mut IoBuf) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn write(inode: *mut INode, buf: *mut IoBuf) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn fstat(inode: *mut INode, stat: *mut Stat) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn fsync(inode: *mut INode) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn namefile(inode: *mut INode, buf: *mut IoBuf) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn getdirentry(inode: *mut INode, buf: *mut IoBuf) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn reclaim(inode: *mut INode) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn gettype(inode: *mut INode, type_store: *mut u32) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn tryseek(inode: *mut INode, pos: i32) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn truncate(inode: *mut INode, len: i32) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn create(inode: *mut INode, name: *const u8, excl: bool, inode_store: *mut *mut INode) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn lookup(inode: *mut INode, path: *mut u8, inode_store: *mut *mut INode) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        extern fn ioctl(inode: *mut INode, op: i32, data: *mut u8) -> ErrorCode {
            ErrorCode::Unimplemented
        }
        INodeOps {
            magic: 0x8c4ba476,
            open, close, read, write, fstat, fsync, namefile, getdirentry,
            reclaim, gettype, tryseek, truncate, create, lookup, ioctl,
        }
    }
}

mod allocator {
    use alloc::heap::{Alloc, AllocErr, Layout};
    use core::ptr::NonNull;

    pub struct UcoreAllocator {
        pub malloc: unsafe extern fn(size: usize) -> *mut u8,
        pub free: unsafe extern fn(*mut u8),
    }

    unsafe impl<'a> Alloc for &'a UcoreAllocator {
        unsafe fn alloc(&mut self, layout: Layout) -> Result<*mut u8, AllocErr> {
            const NULL: *mut u8 = 0 as *mut u8;
            match (self.malloc)(layout.size()) {
                NULL => Err(AllocErr::Exhausted { request: layout }),
                ptr => Ok(ptr),
            }
        }
        unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
            (self.free)(ptr);
        }
    }
}