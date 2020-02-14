//! C Interfaces for ucore
//!
//! NOTE: Must link these sections:
//! `*.got.*` `*.data.*` `*.rodata.*`

#![feature(lang_items)]

#![feature(panic_info_message)]
#![no_std]

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate static_assertions;

use alloc::{boxed::Box, collections::BTreeMap, sync::Arc};
use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::ops::Deref;
use core::slice;
use rcore_fs::{sfs, vfs};
use spin::Mutex;

/// Lang items for bare lib
mod lang {
    use core::alloc::Layout;
    use core::panic::PanicInfo;

    #[lang = "eh_personality"]
    #[no_mangle]
    extern fn eh_personality() {}

    #[lang = "oom"]
    #[no_mangle]
    extern fn oom(_: Layout) -> ! {
        panic!("out of memory");
    }

    #[panic_handler]
    #[no_mangle]
    extern fn panic(info: &PanicInfo) -> ! {
        use super::ucore::__panic;
        let location = info.location().unwrap();
        let message = info.message().unwrap();
        let s = format!("{}\0", message);
        let file = format!("{}\0", location.file());
        unsafe { __panic(file.as_ptr(), location.line() as i32, s.as_ptr()) };
        unreachable!()
    }
}

/// Depends on ucore
mod ucore {
    use super::*;

    extern {
        pub fn kmalloc(size: usize) -> *mut u8;
        pub fn kfree(ptr: *mut u8);

        pub fn inode_kill(inode: &mut INode);
        pub fn inode_get_fs(inode: *mut INode) -> *mut Fs;
        pub fn inode_ref_inc(inode: *mut INode) -> i32;
        pub fn create_inode_for_sfs(ops: &INodeOps, fs: *mut Fs) -> *mut INode;
        pub fn create_fs_for_sfs(ops: &FsOps) -> *mut Fs;

        pub fn __panic(file: *const u8, line: i32, fmt: *const u8, ...);
        pub fn cprintf(fmt: *const u8, ...);
    }
}

#[macro_use]
mod macros {
    macro_rules! cprintf {
        ($fmt:expr) => (unsafe{ ::c_interface::ucore::cprintf(concat!($fmt, "\0").as_ptr()); });
        ($fmt:expr, $($arg:tt)*) => (unsafe{ ::c_interface::ucore::cprintf(concat!($fmt, "\0").as_ptr(), $($arg)*); });
    }

    macro_rules! print {
        ($($arg:tt)*) => (unsafe{ ::c_interface::ucore::cprintf(format!($($arg)*).as_ptr())});
    }

    #[cfg(feature = "debug_print")]
    macro_rules! println {
        () => (print!("\n"));
        ($fmt:expr) => (print!(concat!($fmt, "\n")));
        ($fmt:expr, $($arg:tt)*) => (print!(concat!($fmt, "\n"), $($arg)*));
    }

    #[cfg(not(feature = "debug_print"))]
    macro_rules! println {
        () => ();
        ($fmt:expr) => ();
        ($fmt:expr, $($arg:tt)*) => ();
    }
}

mod libc {
    pub unsafe fn from_cstr(s: *const u8) -> &'static str {
        use core::{str, slice};
        let len = (0usize..).find(|&i| *s.offset(i as isize) == 0).unwrap();
        str::from_utf8(slice::from_raw_parts(s, len)).unwrap()
    }
}

// Exports for ucore

#[no_mangle]
pub extern fn sfs_do_mount(dev: *mut Device, fs_store: &mut *mut Fs) -> ErrorCode {
    let fs = unsafe { ucore::create_fs_for_sfs(&FS_OPS) };
    debug_assert!(!dev.is_null());
    let mut device = unsafe { Box::from_raw(dev) };  // TODO: fix unsafe
    device.open();
    let sfs = sfs::SimpleFileSystem::open(device).unwrap();
    // `fs.fs` is uninitialized, so it must be `replace` out and `forget`
    mem::forget(mem::replace(unsafe { &mut (*fs).fs }, sfs));
    *fs_store = fs;
    ErrorCode::Ok
}

// Structs defined in ucore

/// ﻿Abstract low-level file.
///
/// Match struct `inode` in ucore `kern/fs/vfs/inode.h`
#[repr(C)]
struct INode {
    inode: Arc<vfs::INode>,
    // ... fields handled extern
}

/// ﻿Abstract filesystem. (Or device accessible as a file.)
///
/// Match struct `fs` in ucore `kern/fs/vfs/vfs.h`
#[repr(C)]
pub struct Fs {
    fs: Arc<vfs::FileSystem>,
    // ... fields handled extern
}

/// ﻿A temp structure to pass function pointers to C
///
/// Match struct `fs_ops` in ucore `kern/fs/sfs/sfs.c`
#[repr(C)]
pub struct FsOps {
    /// Flush all dirty buffers to disk
    sync: extern fn(&mut Fs) -> ErrorCode,
    /// Return root inode of filesystem.
    get_root: extern fn(&mut Fs) -> *mut INode,
    /// Attempt unmount of filesystem.
    unmount: extern fn(&mut Fs) -> ErrorCode,
    /// Cleanup of filesystem.???
    cleanup: extern fn(&mut Fs),
}

/// Filesystem-namespace-accessible device.
/// d_io is for both reads and writes; the iobuf will indicates the direction.
///
/// Match struct `device` in ucore `kern/fs/devs/dev.h`
#[repr(C)]
pub struct Device {
    blocks: usize,
    blocksize: usize,
    open: extern fn(&mut Device, flags: OpenFlags) -> ErrorCode,
    close: extern fn(&mut Device) -> ErrorCode,
    io: extern fn(&mut Device, buf: &mut IoBuf, is_write: bool) -> ErrorCode,
    ioctl: extern fn(&mut Device, op: i32, data: *mut u8) -> ErrorCode,
}

bitflags! {
    struct OpenFlags: u32 {
        // flags for open: choose one of these
        const RDONLY = 0         ; // open for reading only
        const WRONLY = 1         ; // open for writing only
        const RDWR   = 2         ; // open for reading and writing;
        // then or in any of these:
        const CREAT  = 0x00000004; // create file if it does not exist
        const EXCL   = 0x00000008; // error if CREAT and the file exists
        const TRUNC  = 0x00000010; // truncate file upon open
        const APPEND = 0x00000020; // append on each write
    }
}

/// ﻿A buffer Rd/Wr status record
///
/// Match struct `iobuf` in ucore `kern/fs/iobuf.h`
#[repr(C)]
#[derive(Debug)]
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
#[derive(Debug)]
struct Stat {
    /// protection mode and file type
    mode: Mode,
    /// number of hard links
    nlinks: u32,
    /// number of blocks file is using
    blocks: u32,
    /// file size (bytes)
    size: u32,
}

bitflags! {
    struct Mode: u32 {
        /// ordinary regular file
        const File = 0o10000;
        /// directory
        const Dir = 0o20000;
        /// symbolic link
        const Link = 0o30000;
        /// character device
        const Char = 0o40000;
        /// block device
        const Block = 0o50000;
    }
}

impl From<vfs::FileType> for Mode {
    fn from(type_: vfs::FileType) -> Self {
        match type_ {
            vfs::FileType::File => Mode::File,
            vfs::FileType::Dir => Mode::Dir,
        }
    }
}

/// ﻿Abstract operations on a inode.
///
/// Match struct `inode_ops` in ucore `kern/fs/vfs/inode.h`
// TODO: Append docs from ucore
#[repr(C)]
pub struct INodeOps {
    magic: u32,
    open: extern fn(&mut INode, flags: u32) -> ErrorCode,
    close: extern fn(&mut INode) -> ErrorCode,
    read: extern fn(&mut INode, &mut IoBuf) -> ErrorCode,
    write: extern fn(&mut INode, &mut IoBuf) -> ErrorCode,
    fstat: extern fn(&mut INode, &mut Stat) -> ErrorCode,
    fsync: extern fn(&mut INode) -> ErrorCode,
    namefile: extern fn(&mut INode, &mut IoBuf) -> ErrorCode,
    getdirentry: extern fn(&mut INode, &mut IoBuf) -> ErrorCode,
    reclaim: extern fn(&mut INode) -> ErrorCode,
    gettype: extern fn(&mut INode, type_store: &mut u32) -> ErrorCode,
    tryseek: extern fn(&mut INode, pos: i32) -> ErrorCode,
    truncate: extern fn(&mut INode, len: i32) -> ErrorCode,
    create: extern fn(&mut INode, name: *const u8, excl: bool, inode_store: &mut *mut INode) -> ErrorCode,
    lookup: extern fn(&mut INode, path: *mut u8, inode_store: &mut *mut INode) -> ErrorCode,
    ioctl: extern fn(&mut INode, op: i32, data: *mut u8) -> ErrorCode,
}

#[repr(i32)]
#[derive(Debug, Eq, PartialEq)]
pub enum ErrorCode {
    /// No error
    Ok = 0,
    /// Unspecified or unknown problem
    UNSPECIFIED = -1,
    /// Process doesn't exist or otherwise
    BAD_PROC = -2,
    /// Invalid parameter
    Invalid = -3,
    /// Request failed due to memory shortage
    NO_MEM = -4,
    /// Attempt to create a new process beyond
    NO_FREE_PROC = -5,
    /// Memory fault
    FAULT = -6,
    /// SWAP READ/WRITE fault
    SWAP_FAULT = -7,
    /// Invalid elf file
    INVAL_ELF = -8,
    /// Process is killed
    KILLED = -9,
    /// Panic Failure
    PANIC = -10,
    /// Timeout
    TIMEOUT = -11,
    /// Argument is Too Big
    TOO_BIG = -12,
    /// No such Device
    NO_DEV = -13,
    /// Device Not Available
    NA_DEV = -14,
    /// Device/File is Busy
    BUSY = -15,
    /// No Such File or Directory
    NoEntry = -16,
    /// Is a Directory
    IsDir = -17,
    /// Not a Directory
    NotDir = -18,
    /// Cross Device-Link
    XDEV = -19,
    /// Unimplemented Feature
    Unimplemented = -20,
    /// Illegal Seek
    SEEK = -21,
    /// Too Many Files are Open
    MAX_OPEN = -22,
    /// File/Directory Already Exists
    EXISTS = -23,
    /// Directory is Not Empty
    NOTEMPTY = -24,
}

// Wrapper functions

impl AsRef<[u8]> for IoBuf {
    fn as_ref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.base, self.resident as usize) }
    }
}

impl AsMut<[u8]> for IoBuf {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.base, self.resident as usize) }
    }
}

impl IoBuf {
    fn skip(&mut self, len: usize) {
        assert!(len as u32 <= self.resident);
        self.base = unsafe { self.base.offset(len as isize) };
        self.offset += len as i32;
        self.resident -= len as u32;
    }
    fn skip_to(&mut self, offset: usize) {
        let now = self.offset as usize;
        self.skip(offset - now);
    }
    fn write(&mut self, data: &[u8]) {
        self.as_mut()[..data.len()].copy_from_slice(data);
        self.skip(data.len());
    }
}

impl vfs::BlockedDevice for Device {
    const BLOCK_SIZE_LOG2: u8 = 12;

    fn read_at(&mut self, block_id: usize, buf: &mut [u8]) -> bool {
        assert!(buf.len() >= 4096);
        let mut io_buf = IoBuf {
            base: buf.as_mut_ptr(),
            offset: (block_id * 4096) as i32,
            len: 4096,
            resident: 4096,
        };
        let ret = (self.io)(self, &mut io_buf, false);
        assert_eq!(ret, ErrorCode::Ok);
        assert_eq!(io_buf.resident, 0);
        true
    }

    fn write_at(&mut self, block_id: usize, buf: &[u8]) -> bool {
        assert!(buf.len() >= 4096);
        let mut io_buf = IoBuf {
            base: buf.as_ptr() as *mut _,
            offset: (block_id * 4096) as i32,
            len: 4096,
            resident: 4096,
        };
        let ret = (self.io)(self, &mut io_buf, true);
        assert_eq!(ret, ErrorCode::Ok);
        assert_eq!(io_buf.resident, 0);
        true
    }
}

impl Device {
    fn open(&mut self) {
        let ret = (self.open)(self, OpenFlags::RDWR);
        assert_eq!(ret, ErrorCode::Ok);
    }
}

lazy_static! {
    // vfs::INode addr -> c::INode addr
    static ref MAPPER: Mutex<BTreeMap<usize, usize>> = Mutex::new(BTreeMap::new());
}

impl INode {
    fn get_or_create(vfs_inode: Arc<vfs::INode>, fs: *mut Fs) -> *mut Self {
        let mut mapper = MAPPER.lock();

        use core::mem::transmute;
        let (addr, _): (usize, usize) = unsafe { transmute(vfs_inode.as_ref()) };
        match mapper.get(&addr) {
            Some(&ptr) => ptr as *mut Self,
            None => {
                let inode = unsafe { ucore::create_inode_for_sfs(&INODE_OPS, fs) };
                assert!(!inode.is_null());
                // `inode.inode` is uninitialized, so it must be `replace` out and `forget`
                mem::forget(mem::replace(unsafe { &mut (*inode).inode }, vfs_inode));
                mapper.insert(addr, inode as usize);
                inode
            }
        }
    }
    fn drop(&mut self) {
        unsafe { ucore::inode_kill(self) };
    }
}

impl From<vfs::FileInfo> for Stat {
    fn from(info: vfs::FileInfo) -> Self {
        Stat {
            mode: Mode::from(info.type_),
            nlinks: info.nlinks as u32,
            blocks: info.blocks as u32,
            size: info.size as u32,
        }
    }
}

static INODE_OPS: INodeOps = {
    impl Deref for INode {
        type Target = Arc<vfs::INode>;

        fn deref(&self) -> &Self::Target {
            &self.inode
        }
    }

    extern fn open(inode: &mut INode, flags: u32) -> ErrorCode {
        println!("inode.open");
        ErrorCode::Ok
    }
    extern fn close(inode: &mut INode) -> ErrorCode {
        println!("inode.close");
        ErrorCode::Ok
    }
    extern fn read(inode: &mut INode, buf: &mut IoBuf) -> ErrorCode {
        println!("inode.read");
        let len = inode.read_at(buf.offset as usize, buf.as_mut()).unwrap();
        buf.skip(len);
        ErrorCode::Ok
    }
    extern fn write(inode: &mut INode, buf: &mut IoBuf) -> ErrorCode {
        println!("inode.write");
        let len = inode.write_at(buf.offset as usize, buf.as_ref()).unwrap();
        buf.skip(len);
        ErrorCode::Ok
    }
    extern fn fstat(inode: &mut INode, stat: &mut Stat) -> ErrorCode {
        println!("inode.fstst {:?}", inode);
        let info = inode.info().unwrap();
        *stat = Stat::from(info);
        ErrorCode::Ok
    }
    extern fn fsync(inode: &mut INode) -> ErrorCode {
        println!("inode.fsync {:?}", inode);
        inode.sync().unwrap();
        ErrorCode::Ok
    }
    extern fn namefile(inode: &mut INode, buf: &mut IoBuf) -> ErrorCode {
        unimplemented!();
    }
    extern fn getdirentry(inode: &mut INode, buf: &mut IoBuf) -> ErrorCode {
        const ENTRY_SIZE: usize = 256;
        println!("inode.getdirentry {:#x?}", buf);
        if inode.info().unwrap().type_ != vfs::FileType::Dir {
            return ErrorCode::NotDir;
        }
        if buf.offset as usize % ENTRY_SIZE != 0 {
            return ErrorCode::Invalid;
        }
        let id = buf.offset as usize / ENTRY_SIZE;
        let names = inode.list().unwrap();
        if id >= names.len() {
            return ErrorCode::NoEntry;
        }
        buf.write(names[id].as_ref());
        buf.write(b"\0");
        buf.skip_to((id + 1) * ENTRY_SIZE);
        ErrorCode::Ok
    }
    extern fn reclaim(inode: &mut INode) -> ErrorCode {
        println!("inode.reclaim: {:?}", inode);
        ErrorCode::Ok
    }
    extern fn gettype(inode: &mut INode, type_store: &mut u32) -> ErrorCode {
        println!("inode.gettype: {:?}", inode);
        let info = inode.info().unwrap();
        // Inconsistent docs in ucore !
        *type_store = Mode::from(info.type_).bits();
        ErrorCode::Ok
    }
    extern fn tryseek(inode: &mut INode, pos: i32) -> ErrorCode {
        println!("inode.tryseek({:?}) at {:?}", pos, inode);
        let fs = inode.fs();
        if pos < 0 || pos as usize >= fs.info().max_file_size {
            return ErrorCode::Invalid;
        }
        let pos = pos as usize;
        let info = inode.info().unwrap();
        if pos > info.size {
            inode.resize(pos).unwrap();
        }
        return ErrorCode::Ok;
    }
    extern fn truncate(inode: &mut INode, len: i32) -> ErrorCode {
        unimplemented!();
    }
    extern fn create(inode: &mut INode, name: *const u8, excl: bool, inode_store: &mut *mut INode) -> ErrorCode {
        unimplemented!();
    }
    extern fn lookup(inode: &mut INode, path: *mut u8, inode_store: &mut *mut INode) -> ErrorCode {
        let path = unsafe { libc::from_cstr(path) };
        println!("inode.lookup({:?}) at {:?}", path, inode);
        let target = inode.lookup(path);
        match target {
            Ok(target) => {
                let fs = unsafe { ucore::inode_get_fs(inode) };
                let inode = INode::get_or_create(target, fs);
                unsafe { ucore::inode_ref_inc(inode) };
                *inode_store = inode;
                ErrorCode::Ok
            }
            Err(_) => ErrorCode::NoEntry,
        }
    }
    extern fn ioctl(inode: &mut INode, op: i32, data: *mut u8) -> ErrorCode {
        unimplemented!();
    }
    INodeOps {
        magic: 0x8c4ba476,
        open, close, read, write, fstat, fsync, namefile, getdirentry,
        reclaim, gettype, tryseek, truncate, create, lookup, ioctl,
    }
};

static FS_OPS: FsOps = {
    impl Deref for Fs {
        type Target = Arc<vfs::FileSystem>;

        fn deref(&self) -> &Self::Target {
            &self.fs
        }
    }

    extern fn sync(fs: &mut Fs) -> ErrorCode {
        println!("fs.sync");
        fs.sync().unwrap();
        ErrorCode::Ok
    }
    extern fn get_root(fs: &mut Fs) -> *mut INode {
        println!("fs.getroot");
        let inode = fs.root_inode();
        INode::get_or_create(inode, fs)
    }
    extern fn unmount(fs: &mut Fs) -> ErrorCode {
        unimplemented!();
    }
    extern fn cleanup(fs: &mut Fs) {
        unimplemented!();
    }
    FsOps { sync, get_root, unmount, cleanup }
};

/// Allocator supported by ucore functions
pub struct UcoreAllocator;

#[global_allocator]
pub static UCORE_ALLOCATOR: UcoreAllocator = UcoreAllocator;

unsafe impl GlobalAlloc for UcoreAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
//        cprintf!("alloc %d\n", layout.size());
        ucore::kmalloc(layout.size())
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
//        cprintf!("free %d\n", layout.size());
        ucore::kfree(ptr);
    }
}

assert_eq_size!(ops; INodeOps, [u8; 64]);