use alloc::rc::{Rc, Weak};

/// ﻿Abstract operations on a inode.
pub trait INode {
    fn open(&mut self, flags: u32) -> Result<(), ()>;
    fn close(&mut self) -> Result<(), ()>;
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;
//    fn fstat(&mut self, buf: &[u8]) -> Result<(), ()>;
    fn sync(&mut self) -> Result<(), ()>;
//    fn name_file(&mut self) -> Result<(), ()>;
//    fn reclaim(&mut self) -> Result<(), ()>;
//    fn get_type(&mut self) -> Result<u32, ()>;
//    fn try_seek(&mut self, offset: u64) -> Result<(), ()>;
//    fn truncate(&mut self, len: u64) -> Result<(), ()>;
//    fn create(&mut self, name: &'static str, excl: bool) -> Result<(), ()>;
//    fn loopup(&mut self, path: &'static str) -> Result<(), ()>;
//    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<(), ()>;
}

/// ﻿Abstract filesystem
pub trait FileSystem {
    type INode: INode;
    fn sync(&mut self) -> Result<(), ()>;
    fn root_inode(&mut self) -> Rc<Self::INode>;
    fn unmount(&mut self) -> Result<(), ()>;
    fn cleanup(&mut self);
}