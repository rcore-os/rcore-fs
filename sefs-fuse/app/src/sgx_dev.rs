use sgx_types::*;
use rcore_fs_sefs::dev::{File, Storage, DevResult, DeviceError};
use std::path::*;
use std::fs::remove_file;

pub struct SgxStorage {
    path: PathBuf,
}

impl SgxStorage {
    pub fn new(eid: sgx_enclave_id_t, path: impl AsRef<Path>) -> Self {
        unsafe { EID = eid; }

        let path_str = path.as_ref().to_str().unwrap();
        let ret = set_sefs_dir(path_str);
        assert_eq!(ret, 0);

        SgxStorage { path: path.as_ref().to_path_buf() }
    }
}

impl Storage for SgxStorage {
    fn open(&self, file_id: usize) -> DevResult<Box<File>> {
        match file_open(file_id, false, &[0u8; 16]) {
            0 => Ok(Box::new(SgxFile { fd: file_id })),
            _ => panic!(),
        }
    }

    fn create(&self, file_id: usize) -> DevResult<Box<File>> {
        match file_open(file_id, true, &[0u8; 16]) {
            0 => Ok(Box::new(SgxFile { fd: file_id })),
            _ => panic!(),
        }
    }

    fn remove(&self, file_id: usize) -> DevResult<()> {
        let mut path = self.path.to_path_buf();
        path.push(format!("{}", file_id));
        match remove_file(path) {
            Ok(_) => Ok(()),
            Err(_) => panic!(),
        }
    }
}

pub struct SgxFile {
    fd: usize,
}

impl File for SgxFile {
    fn read_at(&self, buf: &mut [u8], offset: usize) -> DevResult<usize> {
        match file_read_at(self.fd, offset, buf) {
            size if size > 0 => Ok(size as usize),
            e => panic!("read_at {}", e),
        }
    }

    fn write_at(&self, buf: &[u8], offset: usize) -> DevResult<usize> {
        match file_write_at(self.fd, offset, buf) {
            size if size > 0 => Ok(size as usize),
            _ => panic!(),
        }
    }

    fn set_len(&self, len: usize) -> DevResult<()> {
        match file_set_len(self.fd, len) {
            0 => Ok(()),
            _ => panic!(),
        }
    }

    fn flush(&self) -> DevResult<()> {
        match file_flush(self.fd) {
            0 => Ok(()),
            _ => panic!(),
        }
    }
}

impl Drop for SgxFile {
    fn drop(&mut self) {
        let _ = file_close(self.fd);
    }
}

/// Ecall functions to access SgxFile
extern {
    fn ecall_set_sefs_dir(eid: sgx_enclave_id_t, retval: *mut i32, path: *const u8, len: size_t) -> sgx_status_t;
    fn ecall_file_open(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, create: uint8_t, key: *const sgx_key_128bit_t) -> sgx_status_t;
    fn ecall_file_close(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t) -> sgx_status_t;
    fn ecall_file_flush(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t) -> sgx_status_t;
    fn ecall_file_read_at(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, offset: size_t, buf: *mut uint8_t, len: size_t) -> sgx_status_t;
    fn ecall_file_write_at(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, offset: size_t, buf: *const uint8_t, len: size_t) -> sgx_status_t;
    fn ecall_file_set_len(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, len: size_t) -> sgx_status_t;
}

/// Must be set when init enclave
static mut EID: sgx_enclave_id_t = 0;

fn set_sefs_dir(path: &str) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_set_sefs_dir(EID, &mut ret_val, path.as_ptr(), path.len());
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}

fn file_open(fd: usize, create: bool, key: &sgx_key_128bit_t) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_file_open(EID, &mut ret_val, fd, create as uint8_t, key);
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}

fn file_close(fd: usize) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_file_close(EID, &mut ret_val, fd);
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}

fn file_flush(fd: usize) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_file_flush(EID, &mut ret_val, fd);
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}

fn file_read_at(fd: usize, offset: usize, buf: &mut [u8]) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_file_read_at(EID, &mut ret_val, fd, offset, buf.as_mut_ptr(), buf.len());
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}

fn file_write_at(fd: usize, offset: usize, buf: &[u8]) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_file_write_at(EID, &mut ret_val, fd, offset, buf.as_ptr(), buf.len());
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}

fn file_set_len(fd: usize, len: usize) -> i32 {
    let mut ret_val = -1;
    unsafe {
        let ret = ecall_file_set_len(EID, &mut ret_val, fd, len);
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val
}
