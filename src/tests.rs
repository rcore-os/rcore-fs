use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::boxed::Box;
use super::sfs::*;
use super::vfs::*;
use super::vfs::INode;
use std::rc::Rc;

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

fn _open_sample_file() -> Rc<SimpleFileSystem> {
    let file = File::open("sfs.img")
        .expect("failed to open sfs.img");
    SimpleFileSystem::open(Box::new(file))
        .expect("failed to open SFS")
}

fn _create_new_sfs() -> Rc<SimpleFileSystem> {
    let file = OpenOptions::new()
        .read(true).write(true).create(true).open("test.img")
        .expect("failed to create file");
    SimpleFileSystem::create(Box::new(file), 16 * 4096)
}

#[test]
fn open_sample_file() {
    _open_sample_file();
}

#[test]
fn create_new_sfs() {
    _create_new_sfs();
}

#[test]
fn print_root() {
    let sfs = _open_sample_file();
    let root = sfs.root_inode();
    println!("{:?}", root.borrow());

    use super::structs::{DiskEntry, AsBuf};
    use std::mem::uninitialized;
    let mut entry: DiskEntry = unsafe{uninitialized()};
    for i in 0 .. 23 {
        root.borrow_mut().read_at(i * 4096, entry.as_buf_mut()).unwrap();
        println!("{:?}", entry);
    }
}

#[test]
fn create_file() {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();
    let file1 = root.borrow_mut().create("file1").unwrap();

    assert_eq!(file1.borrow().info().unwrap(), FileInfo {
        size: 0,
        type_: FileType::File,
        mode: 0,
    });

    sfs.sync().unwrap();
}

#[test]
fn lookup() {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();
    let file1 = root.borrow_mut().create("file1").unwrap();
    let found = root.borrow().lookup("file1").expect("lookup not found");
    println!("{:?}", found.borrow());
    println!("{:?}", file1.borrow());
    assert!(Rc::ptr_eq(&found, &file1), "found wrong INode");
    sfs.sync().unwrap();
}