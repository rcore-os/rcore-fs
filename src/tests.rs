use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};
use std::boxed::Box;
use super::sfs::*;

impl Device for File {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Option<usize> {
        match self.seek(SeekFrom::Start(offset)) {
            Ok(real_offset) if real_offset == offset => self.read(buf).ok(),
            _ => None,
        }
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Option<usize> {
        match self.seek(SeekFrom::Start(offset)) {
            Ok(real_offset) if real_offset == offset => self.write(buf).ok(),
            _ => None,
        }
    }
}

#[test]
fn test() {
    let file = File::open("sfs.img")
        .expect("failed to open sfs.img");
    let sfs = SimpleFileSystem::new(Box::new(file))
        .expect("failed to create SFS");
}