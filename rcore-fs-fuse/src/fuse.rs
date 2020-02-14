use fuse::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry,
    ReplyStatfs, ReplyWrite, Request,
};
use rcore_fs::vfs;
use std::collections::btree_map::BTreeMap;
use std::ffi::OsStr;
use std::sync::Arc;
use time::Timespec;

const TTL: Timespec = Timespec { sec: 1, nsec: 0 }; // 1 second

pub struct VfsFuse {
    fs: Arc<dyn vfs::FileSystem>,
    inodes: BTreeMap<usize, Arc<dyn vfs::INode>>,
}

impl VfsFuse {
    pub fn new(fs: Arc<dyn vfs::FileSystem>) -> Self {
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
    fn trans_time_r(time: Timespec) -> vfs::Timespec {
        vfs::Timespec {
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
            uid: 501, // info.uid as u32,
            gid: 20,  // info.gid as u32,
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
        use libc::*;
        match err {
            vfs::FsError::NotSupported => ENOSYS,
            vfs::FsError::EntryNotFound => ENOENT,
            vfs::FsError::EntryExist => EEXIST,
            vfs::FsError::IsDir => EISDIR,
            vfs::FsError::NotFile => EISDIR,
            vfs::FsError::NotDir => ENOTDIR,
            vfs::FsError::NotSameFs => EXDEV,
            vfs::FsError::InvalidParam => EINVAL,
            vfs::FsError::NoDeviceSpace => ENOSPC,
            vfs::FsError::DirRemoved => ENOENT,
            vfs::FsError::DirNotEmpty => ENOTEMPTY,
            vfs::FsError::WrongFs => EINVAL,
            _ => EINVAL,
        }
    }
    fn get_inode(&self, ino: u64) -> vfs::Result<&Arc<dyn vfs::INode>> {
        self.inodes
            .get(&(ino as usize))
            .ok_or(vfs::FsError::EntryNotFound)
    }
}

/// Helper macro to reply error when VFS operation fails
macro_rules! try_vfs {
    ($reply:expr, $expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(err) => {
                $reply.error(Self::trans_error(err));
                return;
            }
        }
    };
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

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<Timespec>,
        mtime: Option<Timespec>,
        _fh: Option<u64>,
        _crtime: Option<Timespec>,
        _chgtime: Option<Timespec>,
        _bkuptime: Option<Timespec>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        if let Some(size) = size {
            try_vfs!(reply, inode.resize(size as usize));
        }
        let mut info = try_vfs!(reply, inode.metadata());
        if let Some(mode) = mode {
            info.mode = mode as u16;
        }
        if let Some(uid) = uid {
            info.uid = uid as usize;
        }
        if let Some(gid) = gid {
            info.gid = gid as usize;
        }
        if let Some(atime) = atime {
            info.atime = Self::trans_time_r(atime);
        }
        if let Some(mtime) = mtime {
            info.mtime = Self::trans_time_r(mtime);
        }
        try_vfs!(reply, inode.set_metadata(&info));
        let attr = Self::trans_attr(info);
        reply.attr(&TTL, &attr);
    }

    fn mknod(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _rdev: u32,
        reply: ReplyEntry,
    ) {
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
        self.inodes.insert(info.inode, target);
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

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEmpty,
    ) {
        let name = name.to_str().unwrap();
        let newname = newname.to_str().unwrap();
        let parent = try_vfs!(reply, self.get_inode(parent));
        let newparent = try_vfs!(reply, self.get_inode(newparent));
        try_vfs!(reply, parent.move_(name, newparent, newname));
        reply.ok();
    }

    fn link(
        &mut self,
        _req: &Request,
        ino: u64,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        let newname = newname.to_str().unwrap();
        let inode = try_vfs!(reply, self.get_inode(ino));
        let newparent = try_vfs!(reply, self.get_inode(newparent));
        try_vfs!(reply, newparent.link(newname, inode));
        let info = try_vfs!(reply, inode.metadata());
        let attr = Self::trans_attr(info);
        reply.entry(&TTL, &attr, 0);
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        reply: ReplyData,
    ) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        let mut data = Vec::<u8>::new();
        data.resize(size as usize, 0);
        try_vfs!(reply, inode.read_at(offset as usize, data.as_mut_slice()));
        reply.data(data.as_slice());
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _flags: u32,
        reply: ReplyWrite,
    ) {
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

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let inode = try_vfs!(reply, self.get_inode(ino));
        for i in offset as usize.. {
            let name = match inode.get_entry(i) {
                Ok(name) => name,
                Err(vfs::FsError::EntryNotFound) => break,
                e @ _ => try_vfs!(reply, e),
            };
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
        reply.statfs(
            info.blocks as u64,
            info.bfree as u64,
            info.bavail as u64,
            info.files as u64,
            info.ffree as u64,
            info.bsize as u32,
            info.namemax as u32,
            info.frsize as u32,
        );
    }
}
