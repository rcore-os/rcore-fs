use alloc::{vec::Vec, string::String, rc::{Rc, Weak}};
use core::cell::RefCell;
use core::mem::size_of;
use core;
use core::fmt::Debug;
use core::any::Any;

/// Interface for FS to read & write
///     TODO: use std::io::{Read, Write}
pub trait Device {
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;
}

/// ﻿Abstract operations on a inode.
pub trait INode: Debug + Any {
    fn open(&mut self, flags: u32) -> Result<()>;
    fn close(&mut self) -> Result<()>;
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn info(&self) -> Result<FileInfo>;
    fn sync(&mut self) -> Result<()>;
//    fn name_file(&mut self) -> Result<()>;
//    fn reclaim(&mut self) -> Result<()>;
    fn resize(&mut self, len: usize) -> Result<()>;
    fn create(&mut self, name: &str, type_: FileType) -> Result<INodePtr>;
    fn unlink(&mut self, name: &str) -> Result<()>;
    /// user of the vfs api should call borrow_mut by itself
    fn link(&mut self, name: &str, other:&mut INode) -> Result<()>;
    fn lookup(&self, path: &str) -> Result<INodePtr>;
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<INodePtr>;
    fn list(&self) -> Result<Vec<String>>;
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self,id: usize) -> Result<String>;
//    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Weak<FileSystem>;
    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any;
    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_mut(&mut self) -> &mut Any;
}

impl INode{
    pub fn downcast_ref<T:INode>(&self) -> Option<&T> {
        self.as_any_ref().downcast_ref::<T>()
    }
    pub fn downcast_mut<T:INode>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut::<T>()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FileInfo {
    pub size: usize,
    pub mode: u32,
    pub type_: FileType,
    pub blocks: usize,
    // Note: different from linux, "." and ".." count in nlinks
    // this is same as original ucore.
    pub nlinks: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub enum FileType {
    File, Dir,
}

#[derive(Debug)]
pub struct FsInfo {
    pub max_file_size: usize,
}

pub type Result<T> = core::result::Result<T, ()>;

/// ﻿Abstract filesystem
pub trait FileSystem {
    fn sync(&self) -> Result<()>;
    fn root_inode(&self) -> INodePtr;
    fn info(&self) -> &'static FsInfo;
//    fn unmount(&self) -> Result<()>;
//    fn cleanup(&self);
}

pub type INodePtr = Rc<RefCell<INode>>;