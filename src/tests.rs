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
    SimpleFileSystem::create(Box::new(file), 32 * 4096)
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
    let file1 = root.borrow_mut().create("file1", FileType::File).unwrap();

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

    assert!(Rc::ptr_eq(&root.borrow().lookup(".").unwrap(), &root), "failed to find .");
    assert!(Rc::ptr_eq(&root.borrow().lookup("..").unwrap(), &root), "failed to find ..");

    let file1 = root.borrow_mut().create("file1", FileType::File)
        .expect("failed to create file1");
    assert!(Rc::ptr_eq(&root.borrow().lookup("file1").unwrap(), &file1), "failed to find file1");
    assert!(root.borrow().lookup("file2").is_err(), "found non-existent file");

    let dir1 = root.borrow_mut().create("dir1", FileType::Dir)
        .expect("failed to create dir1");
    let file2 = dir1.borrow_mut().create("file2", FileType::File)
        .expect("failed to create /dir1/file2");
    assert!(Rc::ptr_eq(&root.borrow().lookup("dir1/file2").unwrap(), &file2), "failed to find dir1/file1");
    assert!(Rc::ptr_eq(&dir1.borrow().lookup("..").unwrap(), &root), "failed to find .. from dir1");

    sfs.sync().unwrap();
}