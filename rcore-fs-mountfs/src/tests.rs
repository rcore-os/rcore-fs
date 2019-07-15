use crate::*;
use rcore_fs::vfs::*;
use rcore_fs_ramfs::RamFS;

#[test]
fn mount() {
    let rootfs = MountFS::new(RamFS::new());
    let root = rootfs.root_inode();
    let mnt = root.create("mnt", FileType::Dir, 0o777).unwrap();

    let ramfs = RamFS::new();
    let root1 = ramfs.root_inode();
    root1.create("file", FileType::File, 0o777).unwrap();

    assert!(mnt.downcast_ref::<MNode>().unwrap().mount(ramfs).is_ok());
    assert!(mnt.find("file").is_ok());
    assert!(root.lookup("mnt/file").is_ok());
}
