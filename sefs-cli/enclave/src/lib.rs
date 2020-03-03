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

#![crate_name = "sefsfuseenclave"]
#![crate_type = "staticlib"]
#![no_std]
#![feature(lang_items)]

#[macro_use]
extern crate log;
//#[macro_use]
//extern crate sgx_tstd;

use self::sgxfs::*;

mod lang;
mod sgxfs;

/// Helper macro to reply error when IO fails
macro_rules! try_io {
    ($expr:expr) => {
        match $expr {
            errno if (errno as i32) == -1 => {
                return errno as i32;
            }
            val => val,
        }
    };
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_open(
    path: *const u8,
    create: bool,
    _integrity_only: i32,
) -> *mut u8 {
    let integrity_only = if _integrity_only != 0 { true } else { false };
    let mode = match create {
        true => "w+b\0",
        false => "r+b\0",
    };
    let file = if !integrity_only {
        sgx_fopen_auto_key(path, mode.as_ptr())
    } else {
        sgx_fopen_integrity_only(path, mode.as_ptr())
    };
    file
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_get_mac(
    file: SGX_FILE,
    mac: *mut sgx_aes_gcm_128bit_tag_t,
) -> i32 {
    sgx_fget_mac(file, mac) as i32
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_close(file: SGX_FILE) -> i32 {
    sgx_fclose(file)
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_flush(file: SGX_FILE) -> i32 {
    sgx_fflush(file)
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_read_at(
    file: SGX_FILE,
    offset: usize,
    buf: *mut u8,
    len: usize,
) -> i32 {
    try_io!(sgx_fseek(file, offset as i64, SEEK_SET));
    sgx_fread(buf, 1, len, file) as i32
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_write_at(
    file: SGX_FILE,
    offset: usize,
    buf: *const u8,
    len: usize,
) -> i32 {
    try_io!(sgx_fseek(file, offset as i64, SEEK_SET));
    sgx_fwrite(buf, 1, len, file) as i32
}

#[no_mangle]
pub unsafe extern "C" fn ecall_file_set_len(file: SGX_FILE, len: usize) -> i32 {
    let current_len = try_io!(sgx_fseek(file, 0, SEEK_END)) as usize;
    if current_len < len {
        static ZEROS: [u8; 0x1000] = [0; 0x1000];
        let mut rest_len = len - current_len;
        while rest_len != 0 {
            let l = rest_len.min(0x1000);
            let ret = try_io!(sgx_fwrite(ZEROS.as_ptr(), 1, l, file)) as i32;
            if ret == -12 {
                warn!("Error 12: \"Cannot allocate memory\". Clear cache and try again.");
                try_io!(sgx_fclear_cache(file));
                try_io!(sgx_fwrite(ZEROS.as_ptr(), 1, l, file));
            } else if ret < 0 {
                return ret;
            }
            rest_len -= l;
        }
        // NOTE: Don't try to write a large slice at once.
        //       It will cause Error 12: "Cannot allocate memory"
    }
    // TODO: how to shrink a file?
    0
}
