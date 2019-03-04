use std::fs;
use std::io::{Read, Write};
use std::mem::uninitialized;
use std::path::Path;
use std::sync::Arc;
use std::error::Error;

use rcore_fs::vfs::{INode, FileType, FsError};

const DEFAULT_MODE: u32 = 0o664;
const BUF_SIZE: usize = 0x1000;

pub fn zip_dir(path: &Path, inode: Arc<INode>) -> Result<(), Box<Error>> {
    let dir = fs::read_dir(path)?;
    for entry in dir {
        let entry = entry?;
        let name_ = entry.file_name();
        let name = name_.to_str().unwrap();
        let type_ = entry.file_type()?;
        if type_.is_file() {
            let inode = inode.create(name, FileType::File, DEFAULT_MODE)?;
            let mut file = fs::File::open(entry.path())?;
            inode.resize(file.metadata()?.len() as usize)?;
            let mut buf: [u8; BUF_SIZE] = unsafe { uninitialized() };
            let mut offset = 0usize;
            let mut len = BUF_SIZE;
            while len == BUF_SIZE {
                len = file.read(&mut buf)?;
                inode.write_at(offset, &buf[..len])?;
                offset += len;
            }
        } else if type_.is_dir() {
            let inode = inode.create(name, FileType::Dir, DEFAULT_MODE)?;
            zip_dir(entry.path().as_path(), inode)?;
        }
    }
    Ok(())
}

pub fn unzip_dir(path: &Path, inode: Arc<INode>) -> Result<(), Box<Error>> {
    let files = inode.list()?;
    for name in files.iter().skip(2) {
        let inode = inode.lookup(name.as_str())?;
        let mut path = path.to_path_buf();
        path.push(name);
        let info = inode.metadata()?;
        match info.type_ {
            FileType::File => {
                let mut file = fs::File::create(&path)?;
                let mut buf: [u8; BUF_SIZE] = unsafe { uninitialized() };
                let mut offset = 0usize;
                let mut len = BUF_SIZE;
                while len == BUF_SIZE {
                    len = inode.read_at(offset, buf.as_mut())?;
                    file.write(&buf[..len])?;
                    offset += len;
                }
            }
            FileType::Dir => {
                fs::create_dir(&path)?;
                unzip_dir(path.as_path(), inode)?;
            }
            _ => panic!("unsupported file type"),
        }
    }
    Ok(())
}
