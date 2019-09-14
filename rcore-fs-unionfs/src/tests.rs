extern crate std;

use crate::UnionFS;
use alloc::sync::Arc;
use rcore_fs::vfs::*;
use rcore_fs_ramfs::RamFS;

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
    let (root, container_root, image_root) = create_sample()?;
    for path in &["file1", "file3", "dir/file4"] {
        const WRITE_DATA: &[u8] = b"I'm writing to container";
        root.lookup(path)?.write_at(0, WRITE_DATA)?;
        assert_eq!(container_root.lookup(path)?.read_as_vec()?, WRITE_DATA);
        assert_eq!(image_root.lookup(path)?.read_as_vec()?, b"image");
    }
    Ok(())
}

#[test]
fn get_direntry() -> Result<()> {
    unimplemented!()
}

const MODE: u32 = 0o777;
