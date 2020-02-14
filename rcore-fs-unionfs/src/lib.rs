#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![deny(warnings)]

extern crate alloc;
extern crate log;

use alloc::{
    collections::BTreeSet,
    string::String,
    sync::{Arc, Weak},
    vec::Vec,
};
use core::any::Any;
use rcore_fs::vfs::*;
use spin::{RwLock, RwLockReadGuard};

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

//type INodeId = usize;

/// INode for `UnionFS`
pub struct UnionINode {
    /// INode ID
    id: usize,
    /// INodes for each inner file systems
    inners: RwLock<Vec<VirtualINode>>,
    /// Cache of merged directory entries.
    cached_entries: RwLock<Vec<String>>,
    /// Path from root INode
    path: Path,
    /// Reference to `UnionFS`
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
            id: 0,
            inners: RwLock::new(
                self.inners
                    .iter()
                    .map(|fs| VirtualINode {
                        last_inode: fs.root_inode(),
                        distance: 0,
                    })
                    .collect(),
            ),
            cached_entries: RwLock::new(Vec::new()),
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
    #[allow(dead_code)]
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
    /// Merge directory entries from several INodes
    fn merge_entries(inners: &[VirtualINode]) -> Result<Vec<String>> {
        let mut entries = BTreeSet::<String>::new();
        // images
        for inode in inners[1..].iter().filter_map(|v| v.as_real()) {
            for name in inode.list()? {
                entries.insert(name);
            }
        }
        // container
        if let Some(inode) = inners[0].as_real() {
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

    /// Get merged directory entries with cache `self.cached_entries`
    fn entries(&self) -> Result<RwLockReadGuard<Vec<String>>> {
        loop {
            let guard = self.cached_entries.read();
            // cache hit
            if guard.len() > 0 {
                return Ok(guard);
            }
            drop(guard);
            // cache miss
            let entries = Self::merge_entries(self.inners.read().as_slice())?;
            *self.cached_entries.write() = entries;
            // next turn will hit
        }
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

    /// Ensure container INode exists in this `UnionINode` and return it.
    ///
    /// If the INode is not exist, first `mkdir -p` the base path.
    /// Then if it is a file, create a copy of the image file;
    /// If it is a directory, create an empty dir.
    fn container_inode(&self) -> Result<Arc<dyn INode>> {
        let type_ = self.inode().metadata()?.type_;
        if type_ != FileType::File && type_ != FileType::Dir {
            return Err(FsError::NotSupported);
        }
        let VirtualINode {
            mut last_inode,
            distance,
        } = self.inners.read()[0].clone();
        if distance == 0 {
            return Ok(last_inode);
        }
        // create dirs to the base path
        for dir_name in &self.path.lastn(distance)[..distance - 1] {
            last_inode = last_inode.create(dir_name, FileType::Dir, 0o777)?;
        }
        // create file or dir
        match type_ {
            FileType::Dir => {
                let dir_name = &self.path.lastn(1)[0];
                last_inode = last_inode.create(dir_name, FileType::Dir, 0o777)?;
            }
            FileType::File => {
                let file_name = &self.path.lastn(1)[0];
                last_inode = last_inode.create(file_name, FileType::File, 0o777)?;
                let data = self.inode().read_as_vec()?;
                last_inode.write_at(0, &data)?;
            }
            _ => unreachable!(),
        }
        self.inners.write()[0] = VirtualINode {
            last_inode: last_inode.clone(),
            distance: 0,
        };
        Ok(last_inode)
    }

    /// Return container INode if it has
    fn maybe_container_inode(&self) -> Option<Arc<dyn INode>> {
        self.inners.read()[0].as_real().map(|v| v.clone())
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
        self.container_inode()?.write_at(offset, buf)
    }

    fn poll(&self) -> Result<PollStatus> {
        self.inode().poll()
    }

    fn metadata(&self) -> Result<Metadata> {
        let mut metadata = self.inode().metadata()?;
        metadata.inode = self.id;
        Ok(metadata)
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
        self.container_inode()?.resize(len)
    }

    fn create(&self, name: &str, type_: FileType, mode: u32) -> Result<Arc<dyn INode>> {
        if self.entries()?.contains(&String::from(name)) {
            return Err(FsError::EntryExist);
        }
        let container_inode = self.container_inode()?;
        match container_inode.unlink(&name.whiteout()) {
            Ok(_) | Err(FsError::EntryNotFound) => {}
            Err(e) => return Err(e),
        }
        let new_inode = container_inode.create(name, type_, mode)?;
        // add `name` to entry cache
        self.cached_entries.write().push(String::from(name));
        Ok(new_inode)
    }

    fn link(&self, name: &str, other: &Arc<dyn INode>) -> Result<()> {
        if self.entries()?.contains(&String::from(name)) {
            return Err(FsError::EntryExist);
        }
        let child = other
            .downcast_ref::<UnionINode>()
            .ok_or(FsError::NotSameFs)?;
        // only support link inside container now
        // TODO: link from image to container
        let child = child.maybe_container_inode().ok_or(FsError::NotSupported)?;
        self.container_inode()?.link(name, &child)?;
        // add `name` to entry cache
        self.cached_entries.write().push(String::from(name));
        Ok(())
    }

    fn unlink(&self, name: &str) -> Result<()> {
        if !self.entries()?.contains(&String::from(name)) {
            return Err(FsError::EntryNotFound);
        }
        // if in container: remove directly
        if let Some(inode) = self.inners.read()[0].as_real() {
            match inode.find(name) {
                Ok(_) => inode.unlink(name)?,
                Err(FsError::EntryNotFound) => {}
                Err(e) => return Err(e),
            }
        }
        // add whiteout to container
        let wh_name = name.whiteout();
        self.container_inode()?
            .create(&wh_name, FileType::File, 0o777)?;
        // remove `name` from entry cache
        self.cached_entries.write().retain(|e| e != name);
        Ok(())
    }

    fn move_(&self, old_name: &str, target: &Arc<dyn INode>, new_name: &str) -> Result<()> {
        // ensure 'old_name' exists in container
        // copy from image on necessary
        self.find(old_name)?
            .downcast_ref::<UnionINode>()
            .unwrap()
            .container_inode()?;
        let target = target
            .downcast_ref::<UnionINode>()
            .ok_or(FsError::NotSameFs)?;
        let this = self.maybe_container_inode().unwrap();
        this.move_(old_name, &target.container_inode()?, new_name)?;
        // add whiteout to container
        this.create(&old_name.whiteout(), FileType::File, 0o777)?;
        // remove `old_name` from entry cache
        self.cached_entries.write().retain(|e| e != old_name);
        // add `new_name` to target's entry cache
        target.cached_entries.write().push(String::from(new_name));
        Ok(())
    }

    fn find(&self, name: &str) -> Result<Arc<dyn INode>> {
        if !self.entries()?.contains(&String::from(name)) {
            return Err(FsError::EntryNotFound);
        }
        let inodes: Result<Vec<_>> = self.inners.read().iter().map(|x| x.find(name)).collect();
        let path = self.path.with_next(name);
        Ok(Arc::new(UnionINode {
            // FIXME: Now INode ID is a hash of its path.
            //        This can avoid conflict when union multiple filesystems,
            //        but it's obviously wrong when the path changes.
            //        We need to find a corrent way to allocate the INode ID.
            id: path.hash(),
            inners: RwLock::new(inodes?),
            cached_entries: RwLock::new(Vec::new()),
            path,
            fs: self.fs.clone(),
        }))
    }

    fn get_entry(&self, id: usize) -> Result<String> {
        let entires = self.entries()?;
        if id >= entires.len() {
            Err(FsError::EntryNotFound)
        } else {
            Ok(entires[id].clone())
        }
    }

    fn io_control(&self, cmd: u32, data: usize) -> Result<()> {
        self.inode().io_control(cmd, data)
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
    /// Hash path to get an INode ID
    fn hash(&self) -> usize {
        // hash function: Times33
        self.0.iter().flat_map(|s| s.bytes()).fold(0usize, |h, b| {
            h.overflowing_mul(33).0.overflowing_add(b as usize).0
        })
    }
}

trait NameExt {
    fn whiteout(&self) -> String;
}

impl NameExt for str {
    fn whiteout(&self) -> String {
        String::from(".wh.") + self
    }
}
