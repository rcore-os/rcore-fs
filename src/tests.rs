use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::boxed::Box;
use super::sfs::*;
use super::vfs::*;

impl Device for File {
    fn read_at(&mut self, offset: usize, buf: &mut [u8]) -> Option<usize> {
        let offset = offset as u64;
        match self.seek(SeekFrom::Start(offset)) {
            Ok(real_offset) if real_offset == offset => self.read(buf).ok(),
            _ => None,
        }
    }

    fn write_at(&mut self, offset: usize, buf: &[u8]) -> Option<usize> {
        let offset = offset as u64;
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
    let sfs = SimpleFileSystem::open(Box::new(file))
        .expect("failed to create SFS");
    let root = sfs.borrow_mut().root_inode();
    println!("{:?}", root);

    use super::structs::{DiskEntry, AsBuf};
    use super::vfs::INode;
    use std::mem::uninitialized;
    let mut entry: DiskEntry = unsafe{uninitialized()};
    for i in 0 .. 23 {
        root.borrow_mut().read_at(i * 4096, entry.as_buf_mut());
        println!("{:?}", entry);
    }
}

#[test]
fn create() {
    let file = OpenOptions::new()
        .read(true).write(true).create(true).open("test.img")
        .expect("failed to create file");
    let sfs = SimpleFileSystem::create(Box::new(file), 16 * 4096);


}