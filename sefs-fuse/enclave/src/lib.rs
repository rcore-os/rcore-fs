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
#[macro_use]
extern crate log;
#[cfg(not(target_env = "sgx"))]
#[macro_use]
extern crate sgx_tstd as std;

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sgxfs::{OpenOptions, SgxFile};
use std::sync::{SgxMutex as Mutex, SgxRwLock as RwLock};
use std::vec::Vec;
use sgx_types::sgx_key_128bit_t;

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

/// Helper macro to reply error when IO fails
macro_rules! try_io {
    ($expr:expr) => (match $expr {
        Ok(val) => val,
        Err(err) => return err.raw_os_error().unwrap(),
    });
}

#[no_mangle]
pub extern "C" fn ecall_file_open(fd: usize, create: bool, key: &sgx_key_128bit_t) -> i32 {
    let path = get_path(fd);
    let mut oo = OpenOptions::new();
    match create {
        true => oo.write(true).update(true).binary(true),
        false => oo.read(true).update(true).binary(true),
    };
    let file = try_io!(oo.open_ex(&path, key));
    debug!("{} fd = {} key = {:?}", if create {"create"} else {"open"}, fd, key);
    let file = LockedFile(Mutex::new(file));
    let mut files = FILES.write().unwrap();
    files.insert(fd, file);
    0
}

#[no_mangle]
pub extern "C" fn ecall_file_close(fd: usize) -> i32 {
    let mut files = FILES.write().unwrap();
    files.remove(&fd);
    debug!("close fd = {}", fd);
    0
}

#[no_mangle]
pub extern "C" fn ecall_file_flush(fd: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();
    debug!("flush fd = {}", fd);
    try_io!(file.flush());
    0
}

#[no_mangle]
pub extern "C" fn ecall_file_read_at(fd: usize, offset: usize, buf: *mut u8, len: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();

    let offset = offset as u64;
    debug!("read_at fd = {}, offset = {}, len = {}", fd, offset, len);
    try_io!(file.seek(SeekFrom::Start(offset)));

    let buf = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    let len = try_io!(file.read(buf)) as i32;
    trace!("{:?}", buf);

    len
}

#[no_mangle]
pub extern "C" fn ecall_file_write_at(fd: usize, offset: usize, buf: *const u8, len: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();

    let offset = offset as u64;
    debug!("write_at fd = {}, offset = {}, len = {}", fd, offset, len);
    try_io!(file.seek(SeekFrom::Start(offset)));
    let buf = unsafe { std::slice::from_raw_parts(buf, len) };
    let ret = try_io!(file.write(buf)) as i32;
    trace!("{:?}", buf);

    ret
}

#[no_mangle]
pub extern "C" fn ecall_file_set_len(fd: usize, len: usize) -> i32 {
    let files = FILES.read().unwrap();
    let mut file = files[&fd].0.lock().unwrap();

    debug!("set_len fd = {}, len = {}", fd, len);
    let current_len = try_io!(file.seek(SeekFrom::End(0))) as usize;
    if current_len < len {
        let mut zeros = Vec::<u8>::new();
        zeros.resize(len - current_len, 0);
        try_io!(file.write(zeros.as_slice()));
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
