#![cfg_attr(not(any(test, feature = "std")), no_std)]

extern crate alloc;
#[macro_use]
extern crate log;

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::any::Any;
use rcore_fs::vfs::*;
use spin::RwLock;

#[cfg(test)]
mod tests;

/// Union File System
///
/// It allows files and directories of separate file systems, known as branches,
/// to be transparently overlaid, forming a single coherent file system.
pub struct UnionFS {
    /// Inner file systems
    /// NOTE: the 1st is RW, others are RO
    inners: Vec<Arc<dyn FileSystem>>,
    /// Weak reference to self
    self_ref: Weak<UnionFS>,
}

type INodeId = usize;

/// INode for `UnionFS`
pub struct UnionINode {
    inners: RwLock<Vec<VirtualINode>>,
    path: Path,
    fs: Arc<dyn FileSystem>,
}

/// A virtual INode of a path in a FS
#[derive(Clone)]
struct VirtualINode {
    /// The last valid INode in the path.
    last_inode: Arc<dyn INode>,
    /// The distance / depth to the last valid INode.
    ///
    /// This should be 0 if the last INode is the current one,
    /// otherwise the path is not exist in the FS, and this is a virtual INode.
    distance: usize,
}

impl UnionFS {
    /// Create a `UnionFS` wrapper for file system `fs`
    pub fn new(fs: Vec<Arc<dyn FileSystem>>) -> Arc<Self> {
        UnionFS {
            inners: fs,
            self_ref: Weak::default(),
        }
        .wrap()
    }

    /// Wrap pure `UnionFS` with `Arc<..>`.
    /// Used in constructors.
    fn wrap(self) -> Arc<Self> {
        // Create an Arc, make a Weak from it, then put it into the struct.
        // It's a little tricky.
        let fs = Arc::new(self);
        let weak = Arc::downgrade(&fs);
        let ptr = Arc::into_raw(fs) as *mut Self;
        unsafe {
            (*ptr).self_ref = weak;
            Arc::from_raw(ptr)
        }
    }

    /// Strong type version of `root_inode`
    pub fn root_inode(&self) -> Arc<UnionINode> {
        Arc::new(UnionINode {
            inners: RwLock::new(
                self.inners
                    .iter()
                    .map(|fs| VirtualINode {
                        last_inode: fs.root_inode(),
                        distance: 0,
                    })
                    .collect(),
            ),
            path: Path::new(),
            fs: self.self_ref.upgrade().unwrap(),
        })
    }
}

impl VirtualINode {
    /// Move this INode to './name'
    fn move_(&mut self, name: &str) -> Result<()> {
        if self.distance == 0 {
            match self.last_inode.find(name) {
                Ok(inode) => self.last_inode = inode,
                Err(FsError::EntryNotFound) => self.distance = 1,
                Err(e) => return Err(e),
            }
        } else {
            match name {
                ".." => self.distance -= 1,
                _ => self.distance += 1,
            }
        }
        Ok(())
    }

    /// Find the next INode at './name'
    fn find(&self, name: &str) -> Result<Self> {
        let mut ret = self.clone();
        ret.move_(name)?;
        Ok(ret)
    }

    /// Whether this is a real INode
    fn is_real(&self) -> bool {
        self.distance == 0
    }

    fn as_real(&self) -> Option<&Arc<dyn INode>> {
        match self.distance {
            0 => Some(&self.last_inode),
            _ => None,
        }
    }
}

impl UnionINode {
    /// Get merged directory entries
    fn entries(&self) -> Result<Vec<String>> {
        let mut entries = BTreeSet::<String>::new();
        // images
        for inode in self.inners.read()[1..].iter().filter_map(|v| v.as_real()) {
            for name in inode.list()? {
                entries.insert(name);
            }
        }
        // container
        if let Some(inode) = self.inners.read()[0].as_real() {
            for name in inode.list()? {
                if name.starts_with(".wh.") {
                    // whiteout
                    entries.remove(&name[4..]);
                } else {
                    entries.insert(name);
                }
            }
        }
        Ok(entries.into_iter().collect())
    }

    /// Determine the upper INode
    fn inode(&self) -> Arc<dyn INode> {
        self.inners
            .read()
            .iter()
            .filter_map(|v| v.as_real())
            .next()
            .unwrap()
            .clone()
    }

    /// Create a copy of image if container INode is not exist
    fn ensure_container_file_exist(&self) -> Result<()> {
        if self.inode().metadata()?.type_ != FileType::File {
            return Err(FsError::NotFile);
        }
        let VirtualINode {
            mut last_inode,
            distance,
        } = self.inners.read()[0].clone();
        if distance == 0 {
            return Ok(());
        }
        for dir_name in &self.path.lastn(distance)[..distance - 1] {
            last_inode = last_inode.create(dir_name, FileType::Dir, 0o777)?;
        }
        let file_name = &self.path.lastn(1)[0];
        last_inode = last_inode.create(file_name, FileType::File, 0o777)?;
        let data = self.inode().read_as_vec()?;
        last_inode.write_at(0, &data)?;
        self.inners.write()[0] = VirtualINode {
            last_inode,
            distance: 0,
        };
        Ok(())
    }
}

impl FileSystem for UnionFS {
    fn sync(&self) -> Result<()> {
        for fs in self.inners.iter() {
            fs.sync()?;
        }
        Ok(())
    }

    fn root_inode(&self) -> Arc<dyn INode> {
        self.root_inode()
    }

    fn info(&self) -> FsInfo {
        unimplemented!()
    }
}

impl INode for UnionINode {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<usize> {
        self.inode().read_at(offset, buf)
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Result<usize> {
        self.ensure_container_file_exist()?;
        self.inode().write_at(offset, buf)
    }

    fn poll(&self) -> Result<PollStatus> {
        self.inode().poll()
    }

    fn metadata(&self) -> Result<Metadata> {
        self.inode().metadata()
    }

    fn set_metadata(&self, metadata: &Metadata) -> Result<()> {
        self.inode().set_metadata(metadata)
    }

    fn sync_all(&self) -> Result<()> {
        if let Some(inode) = self.inners.read()[0].as_real() {
            inode.sync_all()
        } else {
            Ok(())
        }
    }

    fn sync_data(&self) -> Result<()> {
        if let Some(inode) = self.inners.read()[0].as_real() {
            inode.sync_data()
        } else {
            Ok(())
        }
    }

    fn resize(&self, len: usize) -> Result<()> {
        self.ensure_container_file_exist()?;
        self.inode().resize(len)
    }

    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<dyn INode>> {
        unimplemented!()
    }

    fn link(&self, name: &str, other: &Arc<dyn INode>) -> Result<()> {
        unimplemented!()
    }

    fn unlink(&self, name: &str) -> Result<()> {
        unimplemented!()
    }

    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> Result<()> {
        unimplemented!()
    }

    fn find(&self, name: &str) -> Result<Arc<dyn INode>> {
        let inodes: Result<Vec<_>> = self.inners.read().iter().map(|x| x.find(name)).collect();
        Ok(Arc::new(UnionINode {
            inners: RwLock::new(inodes?),
            path: self.path.with_next(name),
            fs: self.fs.clone(),
        }))
    }

    fn get_entry(&self, id: usize) -> Result<String> {
        unimplemented!()
    }

    fn io_control(&self, cmd: u32, data: usize) -> Result<()> {
        unimplemented!()
    }

    fn fs(&self) -> Arc<dyn FileSystem> {
        self.fs.clone()
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }
}

/// Simple path
#[derive(Debug, Clone)]
struct Path(Vec<String>);

impl Path {
    fn new() -> Self {
        Path(Vec::new())
    }
    fn append(&mut self, name: &str) {
        match name {
            "." => {}
            ".." => {
                self.0.pop();
            }
            _ => {
                self.0.push(String::from(name));
            }
        }
    }
    fn with_next(&self, name: &str) -> Self {
        let mut next = self.clone();
        next.append(name);
        next
    }
    fn lastn(&self, n: usize) -> &[String] {
        &self.0[self.0.len() - n..]
    }
}
