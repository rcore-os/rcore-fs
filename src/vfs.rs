use alloc::rc::{Rc, Weak};
use core::cell::RefCell;
use core::mem::size_of;
use core;
use core::fmt::Debug;

/// ﻿Abstract operations on a inode.
pub trait INode: Debug {
    fn open(&mut self, flags: u32) -> Result<()>;
    fn close(&mut self) -> Result<()>;
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn info(&self) -> Result<FileInfo>;
    fn sync(&mut self) -> Result<()>;
//    fn name_file(&mut self) -> Result<()>;
//    fn reclaim(&mut self) -> Result<()>;
//    fn try_seek(&mut self, offset: u64) -> Result<()>;
    fn resize(&mut self, len: usize) -> Result<()>;
    fn create(&mut self, name: &'static str, type_: FileType) -> Result<Rc<RefCell<INode>>>;
    fn lookup(&self, path: &'static str) -> Result<Rc<RefCell<INode>>>;
//    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct FileInfo {
    pub size: usize,
    pub mode: u32,
    pub type_: FileType,
}

#[derive(Debug, Eq, PartialEq)]
pub enum FileType {
    File, Dir,
}

pub type Result<T> = core::result::Result<T, ()>;

/// ﻿Abstract filesystem
pub trait FileSystem {
    fn sync(&self) -> Result<()>;
    fn root_inode(&self) -> Rc<RefCell<INode>>;
//    fn unmount(&self) -> Result<()>;
//    fn cleanup(&self);
}