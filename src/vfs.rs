use alloc::{vec::Vec, string::String, sync::Arc};
use core::fmt::Debug;
use core::any::Any;

/// Interface for FS to read & write
///     TODO: use std::io::{Read, Write}
pub trait Device: Send {
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize>;
    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize>;
}

/// ﻿Abstract operations on a inode.
pub trait INode: Any + Sync + Send {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize>;
    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize>;
    fn info(&self) -> Result<FileInfo>;
    fn sync(&self) -> Result<()>;
    fn resize(&self, len: usize) -> Result<()>;
    fn create(&self, name: &str, type_: FileType) -> Result<Arc<INode>>;
    fn unlink(&self, name: &str) -> Result<()>;
    /// user of the vfs api should call borrow_mut by itself
    fn link(&self, name: &str, other: &Arc<INode>) -> Result<()>;
    fn rename(&self, old_name: &str, new_name: &str) -> Result<()>;
    // when self==target use rename instead since it's not possible to have two mut_ref at the same time.
    fn move_(&self, old_name: &str, target: &Arc<INode>, new_name: &str) -> Result<()>;
    /// lookup with only one layer
    fn find(&self, name: &str) -> Result<Arc<INode>>;
    /// like list()[id]
    /// only get one item in list, often faster than list
    fn get_entry(&self, id: usize) -> Result<String>;
    //    fn io_ctrl(&mut self, op: u32, data: &[u8]) -> Result<()>;
    fn fs(&self) -> Arc<FileSystem>;
    /// this is used to implement dynamics cast
    /// simply return self in the implement of the function
    fn as_any_ref(&self) -> &Any;
}

impl INode {
    pub fn downcast_ref<T: INode>(&self) -> Option<&T> {
        self.as_any_ref().downcast_ref::<T>()
    }
    pub fn list(&self) -> Result<Vec<String>> {
        let info = self.info().unwrap();
        assert_eq!(info.type_, FileType::Dir);
        Ok((0..info.size).map(|i| {
            self.get_entry(i).unwrap()
        }).collect())
    }
    pub fn lookup(&self, path: &str) -> Result<Arc<INode>> {
        assert_eq!(self.info().unwrap().type_, FileType::Dir);
        let mut result = self.find(".")?;
        let mut rest_path = path;
        while rest_path != "" {
            assert_eq!(result.info()?.type_, FileType::Dir);
            let name;
            match rest_path.find('/') {
                None => {
                    name = rest_path;
                    rest_path = ""
                }
                Some(pos) => {
                    name = &rest_path[0..pos];
                    rest_path = &rest_path[pos + 1..]
                }
            };
            match result.find(name) {
                Err(_) => return Err(()),
                Ok(inode) => result = inode,
            };
        }
        Ok(result)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct FileInfo {
    // Note: for normal file size is the actuate file size
    // for directory this is count of dirent.
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
    File,
    Dir,
}

#[derive(Debug)]
pub struct FsInfo {
    pub max_file_size: usize,
}

pub type Result<T> = core::result::Result<T, ()>;

/// ﻿Abstract filesystem
pub trait FileSystem: Sync {
    fn sync(&self) -> Result<()>;
    fn root_inode(&self) -> Arc<INode>;
    fn info(&self) -> &'static FsInfo;
//    fn unmount(&self) -> Result<()>;
//    fn cleanup(&self);
}
