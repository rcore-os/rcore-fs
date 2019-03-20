use fuse::{FileAttr, Filesystem, FileType, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyWrite, ReplyStatfs, Request};
use rcore_fs::vfs;
use std::collections::btree_map::BTreeMap;
use std::ffi::OsStr;
use std::sync::Arc;
//use log::*;
use time::Timespec;

const TTL: Timespec = Timespec { sec: 1, nsec: 0 };                     // 1 second

pub struct VfsFuse {
    fs: Arc<vfs::FileSystem>,
    inodes: BTreeMap<usize, Arc<vfs::INode>>,
}

impl VfsFuse {
    pub fn new(fs: Arc<vfs::FileSystem>) -> Self {
        let mut inodes = BTreeMap::new();
        inodes.insert(1, fs.root_inode());
        VfsFuse { fs, inodes }
    }
    fn trans_time(time: vfs::Timespec) -> Timespec {
        Timespec {
            sec: time.sec,
            nsec: time.nsec,
        }
    }
    fn trans_attr(info: vfs::Metadata) -> FileAttr {
        FileAttr {
            ino: info.inode as u64,
            size: info.size as u64,
            blocks: info.blocks as u64,
            atime: Self::trans_time(info.atime),
            mtime: Self::trans_time(info.mtime),
            ctime: Self::trans_time(info.ctime),
            crtime: Timespec { sec: 0, nsec: 0 },
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
            vfs::FileType::SymLink => FileType::Symlink,
            vfs::FileType::CharDevice => FileType::CharDevice,
            vfs::FileType::BlockDevice => FileType::BlockDevice,
            vfs::FileType::NamedPipe => FileType::NamedPipe,
            vfs::FileType::Socket => FileType::Socket,
        }
    }
    fn trans_error(err: vfs::FsError) -> i32 {
        use vfs::FsError::*;
        use libc::*;
        match err {
            NotSupported => ENOSYS,
            EntryNotFound => ENOENT,
            EntryExist => EEXIST,
            IsDir => EISDIR,
            NotFile => EISDIR,
            NotDir => ENOTDIR,
            NotSameFs => EXDEV,
            InvalidParam => EINVAL,
            NoDeviceSpace => ENOSPC,
            DirRemoved => ENOENT,
            DirNotEmpty => ENOTEMPTY,
            WrongFs => EINVAL,
            _ => EINVAL,
        }
    }
    fn get_inode(&self, ino: u64) -> vfs::Result<&Arc<vfs::INode>> {
        self.inodes.get(&(ino as usize)).ok_or(vfs::FsError::EntryNotFound)
    }
}

/// Helper macro to reply error when VFS operation fails
macro_rules! try_vfs {
    ($reply:expr, $expr:expr) => (match $expr {
        Ok(val) => val,
        Err(err) => {
            $reply.error(Self::trans_error(err));
            return;
        }
    });
}

impl Filesystem for VfsFuse {
    fn destroy(&mut self, _req: &Request) {
        self.inodes.clear();
        self.fs.sync().unwrap();
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let inode = try_vfs!(reply, self.get_inode(parent));
        let target = try_vfs!(reply, inode.lookup(name.to_str().unwrap()));
        let info = try_vfs!(reply, target.metadata());
        self.inodes.insert(info.inode, target);
        let attr = Self::trans_attr(info);
        reply.entry(&TTL, &attr, 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        let info = try_vfs!(reply, inode.metadata());
        let attr = Self::trans_attr(info);
        reply.attr(&TTL, &attr);
    }

    fn mknod(&mut self, _req: &Request, parent: u64, name: &OsStr, mode: u32, _rdev: u32, reply: ReplyEntry) {
        let name = name.to_str().unwrap();
        let inode = try_vfs!(reply, self.get_inode(parent));
        let target = try_vfs!(reply, inode.create(name, vfs::FileType::File, mode));
        let info = try_vfs!(reply, target.metadata());
        self.inodes.insert(info.inode, target);
        let attr = Self::trans_attr(info);
        reply.entry(&TTL, &attr, 0);
    }

    fn mkdir(&mut self, _req: &Request, parent: u64, name: &OsStr, mode: u32, reply: ReplyEntry) {
        let name = name.to_str().unwrap();
        let inode = try_vfs!(reply, self.get_inode(parent));
        let target = try_vfs!(reply, inode.create(name, vfs::FileType::Dir, mode));
        let info = try_vfs!(reply, target.metadata());
        let attr = Self::trans_attr(info);
        reply.entry(&TTL, &attr, 0);
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let name = name.to_str().unwrap();
        let parent = try_vfs!(reply, self.get_inode(parent));
        try_vfs!(reply, parent.unlink(name));
        reply.ok();
    }

    fn rmdir(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.unlink(req, parent, name, reply);
    }

    fn rename(&mut self, _req: &Request, parent: u64, name: &OsStr, newparent: u64, newname: &OsStr, reply: ReplyEmpty) {
        let name = name.to_str().unwrap();
        let newname = newname.to_str().unwrap();
        let parent = try_vfs!(reply, self.get_inode(parent));
        let newparent = try_vfs!(reply, self.get_inode(newparent));
        try_vfs!(reply, parent.move_(name, newparent, newname));
        reply.ok();
    }

    fn link(&mut self, _req: &Request, ino: u64, newparent: u64, newname: &OsStr, reply: ReplyEntry) {
        let newname = newname.to_str().unwrap();
        let inode = try_vfs!(reply, self.get_inode(ino));
        let newparent = try_vfs!(reply, self.get_inode(newparent));
        try_vfs!(reply, newparent.link(newname, inode));
        let info = try_vfs!(reply, inode.metadata());
        let attr = Self::trans_attr(info);
        reply.entry(&TTL, &attr, 0);
    }

    fn read(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, size: u32, reply: ReplyData) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        let mut data = Vec::<u8>::new();
        data.resize(size as usize, 0);
        try_vfs!(reply, inode.read_at(offset as usize, data.as_mut_slice()));
        reply.data(data.as_slice());
    }

    fn write(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, data: &[u8], _flags: u32, reply: ReplyWrite) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        let len = try_vfs!(reply, inode.write_at(offset as usize, data));
        reply.written(len as u32);
    }

    fn flush(&mut self, _req: &Request, ino: u64, _fh: u64, _lock_owner: u64, reply: ReplyEmpty) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        try_vfs!(reply, inode.sync_data());
        reply.ok();
    }

    fn fsync(&mut self, _req: &Request, ino: u64, _fh: u64, datasync: bool, reply: ReplyEmpty) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        if datasync {
            try_vfs!(reply, inode.sync_data());
        } else {
            try_vfs!(reply, inode.sync_all());
        }
        reply.ok();
    }

    fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        let info = try_vfs!(reply, inode.metadata());
        let count = info.size;
        for i in offset as usize..count {
            let name = inode.get_entry(i).unwrap();
            let inode = try_vfs!(reply, inode.find(name.as_str()));
            let info = try_vfs!(reply, inode.metadata());
            let kind = Self::trans_type(info.type_);
            let full = reply.add(info.inode as u64, i as i64 + 1, kind, name);
            if full {
                break;
            }
        }
        reply.ok();
    }

    fn statfs(&mut self, _req: &Request, _ino: u64, reply: ReplyStatfs) {
        let info = self.fs.info();
        reply.statfs(info.blocks as u64, info.bfree as u64, info.bavail as u64,
                     info.files as u64, info.ffree as u64, info.bsize as u32,
                     info.namemax as u32, info.frsize as u32);
    }
}
