use alloc::rc::{Rc, Weak};
use core::cell::RefCell;

/// ﻿Abstract operations on a inode.
pub trait INode {
    fn open(&mut self, flags: u32) -> Result<(), ()>;
    fn close(&mut self) -> Result<(), ()>;
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;
    fn info(&mut self) -> Result<FileInfo, ()>;
    fn sync(&mut self) -> Result<(), ()>;
//    fn name_file(&mut self) -> Result<(), ()>;
//    fn reclaim(&mut self) -> Result<(), ()>;
    fn type_(&self) -> Result<u32, ()>;
//    fn try_seek(&mut self, offset: u64) -> Result<(), ()>;
//    fn truncate(&mut self, len: u64) -> Result<(), ()>;
//    fn create(&mut self, name: &'static str, excl: bool) -> Result<(), ()>;
//    fn loopup(&mut self, path: &'static str) -> Result<(), ()>;
//    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<(), ()>;
}

pub struct FileInfo {
    pub size: usize,
    pub mode: u32,
}

/// ﻿Abstract filesystem
pub trait FileSystem {
    type INode: INode;
    fn sync(&mut self) -> Result<(), ()>;
    fn root_inode(&mut self) -> Rc<RefCell<Self::INode>>;
    fn unmount(&mut self) -> Result<(), ()>;
    fn cleanup(&mut self);
}