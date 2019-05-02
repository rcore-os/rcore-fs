//! A naive cache layer for `BlockDevice`
use super::*;
use alloc::{vec, vec::Vec};
use spin::{Mutex, MutexGuard};

pub struct BlockCache<T: BlockDevice> {
    device: T,
    bufs: Vec<Mutex<Buf>>,
}

struct Buf {
    status: BufStatus,
    data: Vec<u8>,
}

enum BufStatus {
    /// buffer is unused
    Unused,
    /// buffer has been read from disk
    Valid(BlockId),
    /// buffer needs to be written to disk
    Dirty(BlockId),
}

impl<T: BlockDevice> BlockCache<T> {
    pub fn new(device: T, capacity: usize) -> Self {
        let mut bufs = Vec::new();
        bufs.resize_with(capacity, || {
            Mutex::new(Buf {
                status: BufStatus::Unused,
                data: vec![0; 1 << T::BLOCK_SIZE_LOG2 as usize],
            })
        });
        BlockCache { device, bufs }
    }

    /// Get a buffer for `block_id` with any status
    fn get_buf(&self, block_id: BlockId) -> MutexGuard<Buf> {
        for buf in self.bufs.iter() {
            if let Some(lock) = buf.try_lock() {
                match lock.status {
                    BufStatus::Valid(id) if id == block_id => return lock,
                    BufStatus::Dirty(id) if id == block_id => return lock,
                    _ => {}
                }
            }
        }
        self.get_unused()
    }

    /// Get an unused buffer
    fn get_unused(&self) -> MutexGuard<Buf> {
        for buf in self.bufs.iter() {
            if let Some(lock) = buf.try_lock() {
                if let BufStatus::Unused = lock.status {
                    return lock;
                }
            }
        }
        // TODO: choose a victim using LRU
        let mut victim = self.bufs[0].lock();
        self.write_back(&mut victim).expect("failed to write back");
        victim.status = BufStatus::Unused;
        victim
    }

    /// Write back data if buffer is dirty
    fn write_back(&self, buf: &mut Buf) -> Result<()> {
        if let BufStatus::Dirty(block_id) = buf.status {
            self.device.write_at(block_id, &buf.data)?;
            buf.status = BufStatus::Valid(block_id);
        }
        Ok(())
    }
}

impl<T: BlockDevice> Drop for BlockCache<T> {
    fn drop(&mut self) {
        BlockDevice::sync(self).expect("failed to sync");
    }
}

impl<T: BlockDevice> BlockDevice for BlockCache<T> {
    const BLOCK_SIZE_LOG2: u8 = T::BLOCK_SIZE_LOG2;

    fn read_at(&self, block_id: BlockId, buffer: &mut [u8]) -> Result<()> {
        let mut buf = self.get_buf(block_id);
        match buf.status {
            BufStatus::Unused => {
                // read from device
                self.device.read_at(block_id, &mut buf.data)?;
                buf.status = BufStatus::Valid(block_id);
            }
            _ => {}
        }
        buffer[..1 << Self::BLOCK_SIZE_LOG2 as usize].copy_from_slice(&buf.data);
        Ok(())
    }

    fn write_at(&self, block_id: BlockId, buffer: &[u8]) -> Result<()> {
        let mut buf = self.get_buf(block_id);
        buf.status = BufStatus::Dirty(block_id);
        buf.data.copy_from_slice(&buffer[..1 << Self::BLOCK_SIZE_LOG2 as usize]);
        Ok(())
    }

    fn sync(&self) -> Result<()> {
        for buf in self.bufs.iter() {
            self.write_back(&mut buf.lock())?;
        }
        self.device.sync()?;
        Ok(())
    }
}
