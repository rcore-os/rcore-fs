extern crate lib;

use std::env;
use std::fs;
use std::io::Read;
use std::path::Path;
use lib::*;

fn main () {
    println!("USAGE: <zip|unzip> <PATH> <IMG>");
    let args: Vec<_> = env::args().collect();
    let cmd = &args[1];
    let dir_path = Path::new(&args[2]);
    let img_path = Path::new(&args[3]);
    match cmd.as_str() {
        "zip" => zip(dir_path, img_path),
        "unzip" => unzip(dir_path, img_path),
        _ => panic!("Invalid command: {}", cmd),
    }
}

fn zip(path: &Path, img_path: &Path) {
    use lib::std_impl;

    let mut img = fs::File::create(img_path)
        .expect(format!("Failed to create file: {:?}", img_path).as_str());
    let sfs = SimpleFileSystem::create(Box::new(img), 0x1000000);
    let inode = sfs.root_inode();
    zip_dir(path, inode);
}

fn zip_dir(path: &Path, inode: INodePtr) {
    println!("{:?}", path);
    let dir = fs::read_dir(path).expect("Failed to open dir");
    for entry in dir {
        let entry = entry.unwrap();
        let name_ = entry.file_name();
        let name = name_.to_str().unwrap();
        let type_ = entry.file_type().unwrap();
        if type_.is_file() {
            let inode = inode.borrow_mut().create(name, FileType::File)
                .expect("Failed to create INode");
            let mut file = fs::File::open(entry.path())
                .expect("Failed to open file");
            let mut buf = Vec::<u8>::new();
            file.read_to_end(&mut buf)
                .expect("Failed to read file");
            inode.borrow_mut().write_at(0, buf.as_ref())
                .expect("Failed to write image");
            println!("{:?}", entry.path());
        } else if type_.is_dir() {
            let inode = inode.borrow_mut().create(name, FileType::Dir)
                .expect("Failed to create INode");
            zip_dir(entry.path().as_path(), inode);
        }
    }
}

fn unzip(path: &Path, img_path: &Path) {

}