use std::fs::{self, OpenOptions};
use std::boxed::Box;
use std::sync::Arc;
use std::mem::uninitialized;
use crate::sfs::*;
use crate::vfs::*;

fn _open_sample_file() -> Arc<SimpleFileSystem> {
    fs::copy("sfs.img", "test.img").expect("failed to open sfs.img");
    let file = OpenOptions::new()
        .read(true).write(true).open("test.img")
        .expect("failed to open test.img");
    SimpleFileSystem::open(Box::new(file))
        .expect("failed to open SFS")
}

fn _create_new_sfs() -> Arc<SimpleFileSystem> {
    let file = tempfile::tempfile()
        .expect("failed to create file");
    SimpleFileSystem::create(Box::new(file), 32 * 4096)
}

#[test]
#[ignore]
fn open_sample_file() {
    _open_sample_file();
}

#[test]
fn create_new_sfs() {
    let sfs = _create_new_sfs();
    let _root = sfs.root_inode();
}

#[test]
fn create_file() -> Result<()> {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();
    let file1 = root.create("file1", FileType::File)?;

    assert_eq!(file1.info()?, Metadata {
        inode: 5,
        size: 0,
        type_: FileType::File,
        mode: 0o777,
        blocks: 0,
        atime: Timespec { sec: 0, nsec: 0 },
        mtime: Timespec { sec: 0, nsec: 0 },
        nlinks: 1,
        uid: 0,
        ctime: Timespec { sec: 0, nsec: 0 },
        gid: 0
    });

    sfs.sync()?;
    Ok(())
}

#[test]
fn resize() -> Result<()> {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();
    let file1 = root.create("file1", FileType::File)?;
    assert_eq!(file1.info()?.size, 0, "empty file size != 0");

    const SIZE1: usize = 0x1234;
    const SIZE2: usize = 0x1250;
    file1.resize(SIZE1)?;
    assert_eq!(file1.info()?.size, SIZE1, "wrong size after resize");
    let mut data1: [u8; SIZE2] = unsafe { uninitialized() };
    let len = file1.read_at(0, data1.as_mut())?;
    assert_eq!(len, SIZE1, "wrong size returned by read_at()");
    assert_eq!(&data1[..SIZE1], &[0u8; SIZE1][..], "expanded data should be 0");

    sfs.sync()?;
    Ok(())
}

#[test]
fn resize_on_dir_should_panic() -> Result<()> {
   let sfs = _create_new_sfs();
   let root = sfs.root_inode();
   assert!(root.resize(4096).is_err());
   sfs.sync()?;

   Ok(())
}

#[test]
fn resize_too_large_should_panic() -> Result<()> {
   let sfs = _create_new_sfs();
   let root = sfs.root_inode();
   let file1 = root.create("file1", FileType::File)?;
   assert!(file1.resize(1 << 28).is_err());
   sfs.sync()?;

   Ok(())
}

#[test]
fn create_then_lookup() -> Result<()> {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();

    assert!(Arc::ptr_eq(&root.lookup(".")?, &root), "failed to find .");
    assert!(Arc::ptr_eq(&root.lookup("..")?, &root), "failed to find ..");

    let file1 = root.create("file1", FileType::File)
        .expect("failed to create file1");
    assert!(Arc::ptr_eq(&root.lookup("file1")?, &file1), "failed to find file1");
    assert!(root.lookup("file2").is_err(), "found non-existent file");

    let dir1 = root.create("dir1", FileType::Dir)
        .expect("failed to create dir1");
    let file2 = dir1.create("file2", FileType::File)
        .expect("failed to create /dir1/file2");
    assert!(Arc::ptr_eq(&root.lookup("dir1/file2")?, &file2), "failed to find dir1/file1");
    assert!(Arc::ptr_eq(&dir1.lookup("..")?, &root), "failed to find .. from dir1");

    sfs.sync()?;
    Ok(())
}

#[test]
fn arc_layout() {
    // [usize, usize, T]
    //  ^ start       ^ Arc::into_raw
    let p = Arc::new([2u8; 5]);
    let ptr = Arc::into_raw(p);
    let start = unsafe { (ptr as *const usize).offset(-2) };
    let ns = unsafe { &*(start as *const [usize; 2]) };
    assert_eq!(ns, &[1usize, 1]);
}

#[test]
#[ignore]
fn kernel_image_file_create() -> Result<()> {
    let sfs = _open_sample_file();
    let root = sfs.root_inode();
    let files_count_before = root.list()?.len();
    root.create("hello2", FileType::File)?;
    let files_count_after = root.list()?.len();
    assert_eq!(files_count_before + 1, files_count_after);
    assert!(root.lookup("hello2").is_ok());

    sfs.sync()?;
    Ok(())
}

#[test]
#[ignore]
fn kernel_image_file_unlink() -> Result<()> {
    let sfs = _open_sample_file();
    let root = sfs.root_inode();
    let files_count_before = root.list()?.len();
    root.unlink("hello")?;
    let files_count_after = root.list()?.len();
    assert_eq!(files_count_before, files_count_after + 1);
    assert!(root.lookup("hello").is_err());

    sfs.sync()?;
    Ok(())
}

#[test]
#[ignore]
fn kernel_image_file_rename() -> Result<()> {
    let sfs = _open_sample_file();
    let root = sfs.root_inode();
    let files_count_before = root.list()?.len();
    root.rename("hello", "hello2")?;
    let files_count_after = root.list()?.len();
    assert_eq!(files_count_before, files_count_after);
    assert!(root.lookup("hello").is_err());
    assert!(root.lookup("hello2").is_ok());

    sfs.sync()?;
    Ok(())
}

#[test]
#[ignore]
fn kernel_image_file_move() -> Result<()> {
    let sfs = _open_sample_file();
    let root = sfs.root_inode();
    let files_count_before = root.list()?.len();
    root.unlink("divzero")?;
    let rust_dir = root.create("rust", FileType::Dir)?;
    root.move_("hello", &rust_dir, "hello_world")?;
    let files_count_after = root.list()?.len();
    assert_eq!(files_count_before, files_count_after + 1);
    assert!(root.lookup("hello").is_err());
    assert!(root.lookup("divzero").is_err());
    assert!(root.lookup("rust").is_ok());
    assert!(rust_dir.lookup("hello_world").is_ok());

    sfs.sync()?;
    Ok(())
}

#[test]
fn hard_link() -> Result<()> {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();
    let file1 = root.create("file1", FileType::File)?;
    root.link("file2", &file1)?;
    let file2 = root.lookup("file2")?;
    file1.resize(100)?;
    assert_eq!(file2.info()?.size, 100);

    sfs.sync()?;
    Ok(())
}

#[test]
fn nlinks() -> Result<()> {
    let sfs = _create_new_sfs();
    let root = sfs.root_inode();
    // -root
    assert_eq!(root.info()?.nlinks, 2);

    let file1 = root.create("file1", FileType::File)?;
    // -root
    //   `-file1 <f1>
    assert_eq!(file1.info()?.nlinks, 1);
    assert_eq!(root.info()?.nlinks, 2);

    let dir1 = root.create("dir1", FileType::Dir)?;
    // -root
    //   +-dir1
    //   `-file1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(root.info()?.nlinks, 3);

    root.rename("dir1", "dir_1")?;
    // -root
    //   +-dir_1
    //   `-file1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(root.info()?.nlinks, 3);

    dir1.link("file1_", &file1)?;
    // -root
    //   +-dir_1
    //   |  `-file1_ <f1>
    //   `-file1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(root.info()?.nlinks, 3);
    assert_eq!(file1.info()?.nlinks, 2);

    let dir2 = root.create("dir2", FileType::Dir)?;
    // -root
    //   +-dir_1
    //   |  `-file1_ <f1>
    //   +-dir2
    //   `-file1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 2);
    assert_eq!(root.info()?.nlinks, 4);
    assert_eq!(file1.info()?.nlinks, 2);

    root.rename("file1", "file_1")?;
    // -root
    //   +-dir_1
    //   |  `-file1_ <f1>
    //   +-dir2
    //   `-file_1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 2);
    assert_eq!(root.info()?.nlinks, 4);
    assert_eq!(file1.info()?.nlinks, 2);

    root.move_("file_1", &dir2, "file__1")?;
    // -root
    //   +-dir_1
    //   |  `-file1_ <f1>
    //   `-dir2
    //      `-file__1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 2);
    assert_eq!(root.info()?.nlinks, 4);
    assert_eq!(file1.info()?.nlinks, 2);

    root.move_("dir_1", &dir2, "dir__1")?;
    // -root
    //   `-dir2
    //      +-dir__1
    //      |  `-file1_ <f1>
    //      `-file__1 <f1>
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 3);
    assert_eq!(root.info()?.nlinks, 3);
    assert_eq!(file1.info()?.nlinks, 2);

    dir2.unlink("file__1")?;
    // -root
    //   `-dir2
    //      `-dir__1
    //         `-file1_ <f1>
    assert_eq!(file1.info()?.nlinks, 1);
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 3);
    assert_eq!(root.info()?.nlinks, 3);

    dir1.unlink("file1_")?;
    // -root
    //   `-dir2
    //      `-dir__1
    assert_eq!(file1.info()?.nlinks, 0);
    assert_eq!(dir1.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 3);
    assert_eq!(root.info()?.nlinks, 3);

    dir2.unlink("dir__1")?;
    // -root
    //   `-dir2
    assert_eq!(file1.info()?.nlinks, 0);
    assert_eq!(dir1.info()?.nlinks, 0);
    assert_eq!(root.info()?.nlinks, 3);
    assert_eq!(dir2.info()?.nlinks, 2);

    root.unlink("dir2")?;
    // -root
    assert_eq!(file1.info()?.nlinks, 0);
    assert_eq!(dir1.info()?.nlinks, 0);
    assert_eq!(root.info()?.nlinks, 2);
    assert_eq!(dir2.info()?.nlinks, 0);

    sfs.sync()?;
    Ok(())
}