use vfs::{INode, Result};
use alloc::sync::Arc;

pub struct File {
    inode: Arc<INode>,
    offset: usize,
    readable: bool,
    writable: bool,
}

impl File {
    pub fn new(inode: Arc<INode>, readable: bool, writable: bool) -> Self {
        File { inode, offset: 0, readable, writable }
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        assert!(self.readable);
        let len = self.inode.read_at(self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        assert!(self.writable);
        let len = self.inode.write_at(self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }
}