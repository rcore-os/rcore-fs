#![allow(unused)]

extern {
    //
    // sgx_tprotected_fs.h
    //
    pub fn sgx_fopen(filename: * const u8,
                     mode: * const u8,
                     key: * const SGX_KEY) -> SGX_FILE;

    pub fn sgx_fopen_auto_key(filename: * const u8, mode: * const u8) -> SGX_FILE;

    pub fn sgx_fwrite(ptr: * const u8,
                      size: usize,
                      count: usize,
                      stream: SGX_FILE) -> usize;

    pub fn sgx_fread(ptr: * mut u8,
                     size: usize,
                     count: usize,
                     stream: SGX_FILE) -> usize;

    pub fn sgx_ftell(stream: SGX_FILE) -> i64;

    pub fn sgx_fseek(stream: SGX_FILE, offset: i64, origin: i32) -> i32;

    pub fn sgx_fflush(stream: SGX_FILE) -> i32;

    pub fn sgx_ferror(stream: SGX_FILE) -> i32;

    pub fn sgx_feof(stream: SGX_FILE) -> i32;

    pub fn sgx_clearerr(stream: SGX_FILE);

    pub fn sgx_fclose(stream: SGX_FILE) -> i32;

    pub fn sgx_remove(filename: * const u8) -> i32;

    pub fn sgx_fexport_auto_key(filename: * const u8, key: * mut SGX_KEY) -> i32;

    pub fn sgx_fimport_auto_key(filename: * const u8, key: * const SGX_KEY) -> i32;

    pub fn sgx_fclear_cache(stream: SGX_FILE) -> i32;

    #[link_name = "__errno_location"]
    fn errno_location() -> * mut i32;
}

pub type SGX_FILE = *mut u8;
pub type SGX_KEY = [u8; 16];

pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

/// Get the last error number.
pub fn errno() -> i32 {
    unsafe {
        (*errno_location()) as i32
    }
}
