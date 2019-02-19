#![cfg(any(test, feature = "std"))]

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

impl Device for Mutex<File> {
    fn read_at(&self, offset: usize, buf: &mut [u8]) -> Option<usize> {
        let offset = offset as u64;
        let mut file = self.lock().unwrap();
        match file.seek(SeekFrom::Start(offset)) {
            Ok(real_offset) if real_offset == offset => file.read(buf).ok(),
            _ => None,
        }
    }

    fn write_at(&self, offset: usize, buf: &[u8]) -> Option<usize> {
        let offset = offset as u64;
        let mut file = self.lock().unwrap();
        match file.seek(SeekFrom::Start(offset)) {
            Ok(real_offset) if real_offset == offset => file.write(buf).ok(),
            _ => None,
        }
    }
}

pub struct StdTimeProvider;

impl TimeProvider for StdTimeProvider {
    fn current_time(&self) -> Timespec {
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Timespec {
            sec: duration.as_secs() as i64,
            nsec: duration.subsec_nanos() as i32,
        }
    }
}
