use std::collections::btree_map::BTreeMap;
use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Arc;

use fuse::{FileAttr, Filesystem, FileType, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};
use libc;
use structopt::StructOpt;
use time::Timespec;
use log::*;

use simple_filesystem::{sfs, vfs};

const TTL: Timespec = Timespec { sec: 1, nsec: 0 };                     // 1 second

struct VfsWrapper<T: vfs::FileSystem> {
    fs: Arc<T>,
    inodes: BTreeMap<usize, Arc<vfs::INode>>,
}

impl<T: vfs::FileSystem> VfsWrapper<T> {
    fn new(fs: Arc<T>) -> Self {
        let mut inodes = BTreeMap::new();
        inodes.insert(1, fs.root_inode());
        VfsWrapper { fs, inodes }
    }
    fn trans_attr(info: vfs::FileInfo) -> FileAttr {
        FileAttr {
            ino: info.inode as u64,
            size: info.size as u64,
            blocks: info.blocks as u64,
            atime: info.atime,
            mtime: info.mtime,
            ctime: info.ctime,
            crtime: Timespec::new(0, 0),
            kind: Self::trans_type(info.type_),
            perm: info.mode,
            nlink: info.nlinks as u32,
            uid: info.uid as u32,
            gid: info.gid as u32,
            rdev: 0,
            flags: 0,
        }
    }
    fn trans_type(type_: vfs::FileType) -> FileType {
        match type_ {
            vfs::FileType::File => FileType::RegularFile,
            vfs::FileType::Dir => FileType::Directory,
        }
    }
    fn get_inode(&self, ino: u64) -> vfs::Result<&Arc<vfs::INode>> {
        self.inodes.get(&(ino as usize)).ok_or(vfs::FsError::EntryNotFound)
    }
}

macro_rules! try_vfs {
    ($reply:expr, $expr:expr) => (match $expr {
        Ok(val) => val,
        Err(err) => {
            let error_code = match err {
                vfs::FsError::EntryNotFound => libc::ENOENT,
                _ => libc::EINVAL,
            };
            $reply.error(error_code);
            return;
        }
    });
}

impl<T: vfs::FileSystem> Filesystem for VfsWrapper<T> {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        info!("lookup parent={} name={}", parent, name.to_str().unwrap());
        let inode = try_vfs!(reply, self.get_inode(parent));
        let target = try_vfs!(reply, inode.lookup(name.to_str().unwrap()));
        let info = try_vfs!(reply, target.info());
        self.inodes.insert(info.inode, target);
        let attr = Self::trans_attr(info);
        reply.entry(&TTL, &attr, 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        info!("getattr ino={}", ino);
        let inode = try_vfs!(reply, self.get_inode(ino));
        let info = try_vfs!(reply, inode.info());
        let attr = Self::trans_attr(info);
        reply.attr(&TTL, &attr);
    }

    fn read(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, size: u32, reply: ReplyData) {
        info!("read ino={} offset={} size={}", ino, offset, size);
        let inode = try_vfs!(reply, self.get_inode(ino));
        let mut data = Vec::<u8>::new();
        data.resize(size as usize, 0);
        try_vfs!(reply, inode.read_at(offset as usize, data.as_mut_slice()));
        reply.data(data.as_slice());
    }

    fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        info!("readdir ino={}, offset={}", ino, offset);
        let inode = try_vfs!(reply, self.get_inode(ino));
        let info = try_vfs!(reply, inode.info());
        let count = info.size;
        for i in offset as usize..count {
            let name = inode.get_entry(i).unwrap();
            let inode = try_vfs!(reply, inode.find(name.as_str()));
            let info = try_vfs!(reply, inode.info());
            let kind = Self::trans_type(info.type_);
            let full = reply.add(info.inode as u64, i as i64 + 1, kind, name);
            if full {
                break;
            }
        }
        reply.ok();
    }
}

#[derive(Debug, StructOpt)]
struct Opt {
    /// Image file
    #[structopt(parse(from_os_str))]
    image: PathBuf,
    /// Mount point
    #[structopt(parse(from_os_str))]
    mount_point: PathBuf,
}

fn main() {
    env_logger::init().unwrap();
    let opt = Opt::from_args();
    let img = OpenOptions::new().read(true).write(true).open(&opt.image)
        .expect("failed to open image");
    let sfs = sfs::SimpleFileSystem::open(Box::new(img))
        .expect("failed to open sfs");
    fuse::mount(VfsWrapper::new(sfs), &opt.mount_point, &[])
        .expect("failed to mount fs");
}
