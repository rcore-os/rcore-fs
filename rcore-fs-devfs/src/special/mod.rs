//! Built-in special device files

use super::*;

macro_rules! impl_inode {
    () => {
        fn set_metadata(&self, _metadata: &Metadata) -> Result<()> { Ok(()) }
        fn sync_all(&self) -> Result<()> { Ok(()) }
        fn sync_data(&self) -> Result<()> { Ok(()) }
        fn resize(&self, _len: usize) -> Result<()> { Err(FsError::NotSupported) }
        fn create(&self, _name: &str, _type_: FileType, _mode: u32) -> Result<Arc<dyn INode>> { Err(FsError::NotDir) }
        fn unlink(&self, _name: &str) -> Result<()> { Err(FsError::NotDir) }
        fn link(&self, _name: &str, _other: &Arc<dyn INode>) -> Result<()> { Err(FsError::NotDir) }
        fn move_(&self, _old_name: &str, _target: &Arc<dyn INode>, _new_name: &str) -> Result<()> { Err(FsError::NotDir) }
        fn find(&self, _name: &str) -> Result<Arc<dyn INode>> { Err(FsError::NotDir) }
        fn get_entry(&self, _id: usize) -> Result<String> { Err(FsError::NotDir) }
        fn io_control(&self, _cmd: u32, _data: usize) -> Result<()> { Err(FsError::NotSupported) }
        fn fs(&self) -> Arc<dyn FileSystem> { unimplemented!() }
        fn as_any_ref(&self) -> &dyn Any { self }
    };
}

mod zero;
mod null;

pub use self::zero::*;
pub use self::null::*;