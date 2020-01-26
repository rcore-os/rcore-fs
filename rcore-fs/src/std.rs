use crate::vfs::*;
use std::io::Error;
use std::os::unix::fs::MetadataExt;

impl std::error::Error for FsError {}

impl From<std::io::Error> for FsError {
    fn from(e: Error) -> Self {
        use std::io::ErrorKind;
        match e.kind() {
            ErrorKind::NotFound => FsError::EntryNotFound,
            ErrorKind::AlreadyExists => FsError::EntryExist,
            ErrorKind::WouldBlock => FsError::Again,
            ErrorKind::InvalidInput => FsError::InvalidParam,
            ErrorKind::InvalidData => FsError::InvalidParam,
            _ => unimplemented!(),
        }
    }
}

impl From<std::fs::Metadata> for Metadata {
    fn from(m: std::fs::Metadata) -> Self {
        Metadata {
            dev: m.dev() as usize,
            inode: m.ino() as usize,
            size: m.size() as usize,
            blk_size: m.blksize() as usize,
            blocks: m.blocks() as usize,
            atime: Timespec {
                sec: m.atime(),
                nsec: m.atime_nsec() as i32,
            },
            mtime: Timespec {
                sec: m.mtime(),
                nsec: m.mtime_nsec() as i32,
            },
            ctime: Timespec {
                sec: m.ctime(),
                nsec: m.ctime_nsec() as i32,
            },
            type_: match (m.mode() & 0xf000) as _ {
                libc::S_IFCHR => FileType::CharDevice,
                libc::S_IFBLK => FileType::BlockDevice,
                libc::S_IFDIR => FileType::Dir,
                libc::S_IFREG => FileType::File,
                libc::S_IFLNK => FileType::SymLink,
                libc::S_IFSOCK => FileType::Socket,
                _ => unimplemented!("unknown file type"),
            },
            mode: m.mode() as u16 & 0o777,
            nlinks: m.nlink() as usize,
            uid: m.uid() as usize,
            gid: m.gid() as usize,
            rdev: m.rdev() as usize,
        }
    }
}
