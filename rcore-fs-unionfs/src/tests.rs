extern crate std;

use crate::UnionFS;
use alloc::sync::Arc;
use rcore_fs::vfs::*;
use rcore_fs_ramfs::RamFS;
use std::collections::btree_set::BTreeSet;

/// Create a UnionFS for test.
/// Return root INode of (union, container, image).
///
/// container:
/// ├── file1
/// └── file2
/// image:
/// ├── file1
/// ├── file3
/// └── dir
///     └── file4
fn create_sample() -> Result<(Arc<dyn INode>, Arc<dyn INode>, Arc<dyn INode>)> {
    let container_fs = {
        let fs = RamFS::new();
        let root = fs.root_inode();
        let file1 = root.create("file1", FileType::File, MODE)?;
        let file2 = root.create("file2", FileType::File, MODE)?;
        file1.write_at(0, b"container")?;
        file2.write_at(0, b"container")?;
        fs
    };
    let container_root = container_fs.root_inode();

    let image_fs = {
        let fs = RamFS::new();
        let root = fs.root_inode();
        let file1 = root.create("file1", FileType::File, MODE)?;
        let file3 = root.create("file3", FileType::File, MODE)?;
        let dir = root.create("dir", FileType::Dir, MODE)?;
        let file4 = dir.create("file4", FileType::File, MODE)?;
        file1.write_at(0, b"image")?;
        file3.write_at(0, b"image")?;
        file4.write_at(0, b"image")?;
        fs
    };
    let image_root = image_fs.root_inode();

    let unionfs = UnionFS::new(vec![container_fs, image_fs]);
    let union_root = unionfs.root_inode();

    Ok((union_root, container_root, image_root))
}

#[test]
fn read_file() -> Result<()> {
    let (root, _, _) = create_sample()?;
    assert_eq!(root.lookup("file1")?.read_as_vec()?, b"container");
    assert_eq!(root.lookup("file2")?.read_as_vec()?, b"container");
    assert_eq!(root.lookup("file3")?.read_as_vec()?, b"image");
    assert_eq!(root.lookup("dir/file4")?.read_as_vec()?, b"image");
    Ok(())
}

#[test]
fn write_file() -> Result<()> {
    let (root, croot, iroot) = create_sample()?;
    for path in &["file1", "file3", "dir/file4"] {
        const WRITE_DATA: &[u8] = b"I'm writing to container";
        root.lookup(path)?.write_at(0, WRITE_DATA)?;
        assert_eq!(croot.lookup(path)?.read_as_vec()?, WRITE_DATA);
        assert_eq!(iroot.lookup(path)?.read_as_vec()?, b"image");
    }
    Ok(())
}

#[test]
fn get_direntry() -> Result<()> {
    let (root, croot, iroot) = create_sample()?;
    let entries: BTreeSet<String> = root.list()?.into_iter().collect();
    let expected: BTreeSet<String> = [".", "..", "file1", "file2", "file3", "dir"]
        .iter()
        .map(|&s| String::from(s))
        .collect();
    assert_eq!(entries, expected);
    Ok(())
}

#[test]
fn unlink() -> Result<()> {
    let (root, croot, iroot) = create_sample()?;

    root.unlink("file1")?;
    assert!(root.lookup("file1").is_not_found());
    assert!(croot.lookup("file1").is_not_found());
    assert!(croot.lookup(".wh.file1").is_ok());
    assert!(iroot.lookup("file1").is_ok());

    root.unlink("file2")?;
    assert!(root.lookup("file2").is_not_found());
    assert!(croot.lookup("file2").is_not_found());

    root.unlink("file3")?;
    assert!(root.lookup("file3").is_not_found());
    assert!(croot.lookup(".wh.file3").is_ok());
    assert!(iroot.lookup("file3").is_ok());

    root.lookup("dir")?.unlink("file4")?;
    assert!(root.lookup("dir/file4").is_not_found());
    assert!(croot.lookup("dir/.wh.file4").is_ok());
    assert!(iroot.lookup("dir/file4").is_ok());

    Ok(())
}

#[test]
fn unlink_then_create() -> Result<()> {
    let (root, croot, iroot) = create_sample()?;
    root.unlink("file1")?;
    let file1 = root.create("file1", FileType::File, MODE)?;
    assert_eq!(file1.read_as_vec()?, b"");
    assert!(croot.lookup(".wh.file1").is_not_found());
    Ok(())
}

const MODE: u32 = 0o777;

trait IsNotFound {
    fn is_not_found(&self) -> bool;
}

impl<T> IsNotFound for Result<T> {
    fn is_not_found(&self) -> bool {
        match self {
            Err(FsError::EntryNotFound) => true,
            _ => false,
        }
    }
}
