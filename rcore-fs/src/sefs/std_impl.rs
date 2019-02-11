#![cfg(any(test, feature = "std"))]

use std::path::{Path, PathBuf};
use std::fs::{File, OpenOptions, remove_file};
use std::io::{Read, Write, Seek, SeekFrom};
use spin::Mutex;
use super::{DeviceError, DevResult};

pub struct StdStorage {
    path: PathBuf,
}

impl StdStorage {
    pub fn new(path: impl AsRef<Path>) -> Self {
        assert!(path.as_ref().is_dir());
        StdStorage { path: path.as_ref().to_path_buf() }
    }
}

impl super::Storage for StdStorage {
    fn open(&self, file_id: usize) -> DevResult<Box<super::File>> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Box::new(Mutex::new(file)))
    }

    fn create(&self, file_id: usize) -> DevResult<Box<super::File>> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        let file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
        Ok(Box::new(Mutex::new(file)))
    }

    fn remove(&self, file_id: usize) -> DevResult<()> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        remove_file(path)?;
        Ok(())
    }
}

impl From<std::io::Error> for DeviceError {
    fn from(e: std::io::Error) -> Self {
        panic!("{:?}", e);
        DeviceError
    }
}

impl super::File for Mutex<File> {
    fn read_at(&self, buf: &mut [u8], offset: usize) -> DevResult<usize> {
        let mut file = self.lock();
        let offset = offset as u64;
        let real_offset = file.seek(SeekFrom::Start(offset))?;
        if real_offset != offset {
            return Err(DeviceError);
        }
        let len = file.read(buf)?;
        Ok(len)
    }

    fn write_at(&self, buf: &[u8], offset: usize) -> DevResult<usize> {
        let mut file = self.lock();
        let offset = offset as u64;
        let real_offset = file.seek(SeekFrom::Start(offset))?;
        if real_offset != offset {
            return Err(DeviceError);
        }
        let len = file.write(buf)?;
        Ok(len)
    }

    fn set_len(&self, len: usize) -> DevResult<()> {
        let mut file = self.lock();
        file.set_len(len as u64)?;
        Ok(())
    }

    fn flush(&self) -> DevResult<()> {
        let mut file = self.lock();
        file.sync_all()?;
        Ok(())
    }
}
