#![cfg(feature = "sgx")]

use std::boxed::Box;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sgxfs::{OpenOptions, remove, SgxFile as File};
use std::sync::SgxMutex as Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rcore_fs::dev::TimeProvider;
use rcore_fs::vfs::Timespec;

use super::{DeviceError, DevResult};

pub struct StdStorage {
    path: PathBuf,
}

impl StdStorage {
    pub fn new(path: impl AsRef<Path>) -> Self {
//        assert!(path.as_ref().is_dir());
        StdStorage { path: path.as_ref().to_path_buf() }
    }
}

impl super::Storage for StdStorage {
    fn open(&self, file_id: usize) -> DevResult<Box<super::File>> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Box::new(LockedFile(Mutex::new(file))))
    }

    fn create(&self, file_id: usize) -> DevResult<Box<super::File>> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Box::new(LockedFile(Mutex::new(file))))
    }

    fn remove(&self, file_id: usize) -> DevResult<()> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        remove(path)?;
        Ok(())
    }
}

impl From<std::io::Error> for DeviceError {
    fn from(e: std::io::Error) -> Self {
        panic!("{:?}", e);
        DeviceError
    }
}

pub struct LockedFile(Mutex<File>);

// `sgx_tstd::sgxfs::SgxFile` not impl Send ...
unsafe impl Send for LockedFile {}
unsafe impl Sync for LockedFile {}

impl super::File for LockedFile {
    fn read_at(&self, buf: &mut [u8], offset: usize) -> DevResult<usize> {
        let mut file = self.0.lock().unwrap();
        let offset = offset as u64;
        let real_offset = file.seek(SeekFrom::Start(offset))?;
        if real_offset != offset {
            return Err(DeviceError);
        }
        let len = file.read(buf)?;
        Ok(len)
    }

    fn write_at(&self, buf: &[u8], offset: usize) -> DevResult<usize> {
        let mut file = self.0.lock().unwrap();
        let offset = offset as u64;
        let real_offset = file.seek(SeekFrom::Start(offset))?;
        if real_offset != offset {
            return Err(DeviceError);
        }
        let len = file.write(buf)?;
        Ok(len)
    }

    fn set_len(&self, len: usize) -> DevResult<()> {
        // NOTE: do nothing ??
        Ok(())
    }

    fn flush(&self) -> DevResult<()> {
        let mut file = self.0.lock().unwrap();
        file.flush()?;
        Ok(())
    }
}

pub struct SgxTimeProvider;

impl TimeProvider for StdTimeProvider {
    fn current_time(&self) -> Timespec {
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timespec {
            sec: duration.as_secs() as i64,
            nsec: duration.subsec_nanos() as i32,
        }
    }
}
