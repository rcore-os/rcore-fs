// Copyright (C) 2017-2018 Baidu, Inc. All Rights Reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
//
//  * Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
//  * Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in
//    the documentation and/or other materials provided with the
//    distribution.
//  * Neither the name of Baidu, Inc., nor the names of its
//    contributors may be used to endorse or promote products derived
//    from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#![crate_name = "helloworldsampleenclave"]
#![crate_type = "staticlib"]

#![cfg_attr(not(target_env = "sgx"), no_std)]
#![cfg_attr(target_env = "sgx", feature(rustc_private))]

#[macro_use]
extern crate lazy_static;
#[cfg(not(target_env = "sgx"))]
#[macro_use]
extern crate sgx_tstd as std;
extern crate sgx_types;

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sgxfs::{OpenOptions, SgxFile};
use std::sync::{SgxMutex as Mutex, SgxRwLock as RwLock};
use std::untrusted::path::PathEx;
use std::vec::Vec;

#[no_mangle]
pub extern "C" fn ecall_set_sefs_dir(path: *const u8, len: usize) -> i32 {
    unsafe {
        assert!(PATH.is_none());
        let path = std::slice::from_raw_parts(path, len);
        let path = std::str::from_utf8(path).unwrap();
        let path = PathBuf::from(path);
        PATH = Some(path);
        0
    }
}

#[no_mangle]
pub extern "C" fn ecall_file_open(fd: usize) -> i32 {
    let path = get_path(fd);
    let file = match OpenOptions::new().append(true).update(true).open(&path) {
        Ok(f) => f,
        Err(e) => { println!("open err {}", e); panic!() }
    };
    let file = LockedFile(Mutex::new(file));
    let mut files = FILES.write().unwrap();
    files.insert(fd, file);
    0
}

#[no_mangle]
pub extern "C" fn ecall_file_close(fd: usize) -> i32 {
    let mut files = FILES.write().unwrap();
    files.remove(&fd);
    0
}

#[no_mangle]
pub extern "C" fn ecall_file_flush(fd: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();
    match file.flush() {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn ecall_file_read_at(fd: usize, offset: usize, buf: *mut u8, len: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();

    println!("read_at fd = {}, offset = {}, len = {}", fd, offset, len);
    let offset = offset as u64;
    match file.seek(SeekFrom::Start(offset)) {
        Ok(real_offset) if real_offset == offset => {},
        _ => return -1,
    }
    let buf = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    match file.read(buf) {
        Ok(len) => len as i32,
        Err(e) => {println!("read_at fail {}", e); -2},
    }
}

#[no_mangle]
pub extern "C" fn ecall_file_write_at(fd: usize, offset: usize, buf: *const u8, len: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();

    let offset = offset as u64;
    println!("write_at fd = {}, offset = {}, len = {}", fd, offset, len);
    match file.seek(SeekFrom::Start(offset)) {
        Ok(real_offset) if real_offset == offset => {},
        _ => return -1,
    }
    let buf = unsafe { std::slice::from_raw_parts(buf, len) };
    match file.write(buf) {
        Ok(len) => len as i32,
        Err(_) => return -2,
    }
}

#[no_mangle]
pub extern "C" fn ecall_file_set_len(fd: usize, len: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();

    println!("set_len fd = {}, len = {}", fd, len);
    let current_len = match file.seek(SeekFrom::End(0)) {
        Ok(len) => len as usize,
        Err(_) => return -1,
    };
    if current_len < len {
        let mut zeros = Vec::<u8>::new();
        zeros.resize(len - current_len, 0);
        match file.write(zeros.as_slice()) {
            Ok(_) => {}
            Err(_) => return -2,
        }
    }
    // TODO: how to shrink a file?
    0
}

static mut PATH: Option<PathBuf> = None;
lazy_static! {
    static ref FILES: RwLock<BTreeMap<usize, LockedFile>> = RwLock::new(BTreeMap::new());
}

struct LockedFile(Mutex<SgxFile>);

// `sgx_tstd::sgxfs::SgxFile` not impl Send ...
unsafe impl Send for LockedFile {}
unsafe impl Sync for LockedFile {}

/// Get file path of `fd`.
///
/// `ecall_set_sefs_dir` must be called first, or this will panic.
fn get_path(fd: usize) -> PathBuf {
    let mut path = unsafe { PATH.as_ref().unwrap().clone() };
    path.push(format!("{}", fd));
    path
}
