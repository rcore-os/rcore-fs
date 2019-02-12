#![cfg(any(test, feature = "std"))]

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Mutex;

use super::Device;

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