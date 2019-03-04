use std::fs;
use std::io::{Read, Result, Write};
use std::mem::uninitialized;
use std::path::Path;
use std::sync::Arc;

use rcore_fs::vfs::*;

const DEFAULT_MODE: u32 = 0o664;

pub fn zip_dir(path: &Path, inode: Arc<INode>) -> Result<()> {
    let dir = fs::read_dir(path).expect("Failed to open dir");
    for entry in dir {
        let entry = entry?;
        let name_ = entry.file_name();
        let name = name_.to_str().unwrap();
        let type_ = entry.file_type()?;
        if type_.is_file() {
            let inode = inode.create(name, FileType::File, DEFAULT_MODE).expect("Failed to create INode");
            let mut file = fs::File::open(entry.path())?;
            inode.resize(file.metadata().unwrap().len() as usize).expect("Failed to resize INode");
            let mut buf: [u8; 4096] = unsafe { uninitialized() };
            let mut offset = 0usize;
            let mut len = 4096;
            while len == 4096 {
                len = file.read(&mut buf)?;
                inode.write_at(offset, &buf).expect("Failed to write image");
                offset += len;
            }
        } else if type_.is_dir() {
            let inode = inode.create(name, FileType::Dir, DEFAULT_MODE).expect("Failed to create INode");
            zip_dir(entry.path().as_path(), inode)?;
        }
    }
    Ok(())
}

pub fn unzip_dir(path: &Path, inode: Arc<INode>) -> Result<()> {
    let files = inode.list().expect("Failed to list files from INode");
    for name in files.iter().skip(2) {
        let inode = inode.lookup(name.as_str()).expect("Failed to lookup");
        let mut path = path.to_path_buf();
        path.push(name);
        let info = inode.metadata().expect("Failed to get file info");
        match info.type_ {
            FileType::File => {
                let mut file = fs::File::create(&path)?;
                let mut buf: [u8; 4096] = unsafe { uninitialized() };
                let mut offset = 0usize;
                let mut len = 4096;
                while len == 4096 {
                    len = inode.read_at(offset, buf.as_mut()).expect("Failed to read from INode");
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
