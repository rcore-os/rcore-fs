extern crate std;

use crate::*;
use std::fs::{self, OpenOptions};
use std::sync::Arc;
use std::sync::Mutex;

fn open_sample_file() -> Arc<Ext2FileSystem> {
    fs::copy("ext2.img", "test.img").expect("failed to open ext2.img");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open("test.img")
        .expect("failed to open test.img");
    Ext2FileSystem::open(Arc::new(Mutex::new(file))).expect("failed to open Ext2")
}

#[test]
fn test_open() {
    let _ = open_sample_file();
}
