use sgx_types::*;
use rcore_fs_sefs::dev::{File, Storage, DevResult};
use std::path::*;
use std::fs::remove_file;
use rcore_fs_sefs::dev::{SefsMac};
use std::mem;

pub struct SgxStorage {
    path: PathBuf,
    integrity_only: bool,
}

impl SgxStorage {
    pub fn new(
        eid: sgx_enclave_id_t,
        path: impl AsRef<Path>,
        integrity_only: bool,
    ) -> Self {
        unsafe { EID = eid; }
        SgxStorage {
            path: path.as_ref().to_path_buf(),
            integrity_only: integrity_only,
        }
    }
}

impl Storage for SgxStorage {
    fn open(&self, file_id: &str) -> DevResult<Box<dyn File>> {
        let mut path = self.path.clone();
        path.push(file_id);
        let file = file_open(path.to_str().unwrap(), false, self.integrity_only);
        Ok(Box::new(SgxFile { file }))
    }

    fn create(&self, file_id: &str) -> DevResult<Box<dyn File>> {
        let mut path = self.path.clone();
        path.push(file_id);
        let file = file_open(path.to_str().unwrap(), true, self.integrity_only);
        Ok(Box::new(SgxFile { file }))
    }

    fn remove(&self, file_id: &str) -> DevResult<()> {
        let mut path = self.path.to_path_buf();
        path.push(file_id);
        match remove_file(path) {
            Ok(_) => Ok(()),
            Err(_) => panic!(),
        }
    }
    fn is_integrity_only(&self) -> bool {
        self.integrity_only
    }
}

pub struct SgxFile {
    file: usize,
}

impl File for SgxFile {
    fn read_at(&self, buf: &mut [u8], offset: usize) -> DevResult<usize> {
        match file_read_at(self.file, offset, buf) {
            size if size >= 0 => Ok(size as usize),
            e => panic!("read_at {}", e),
        }
    }

    fn write_at(&self, buf: &[u8], offset: usize) -> DevResult<usize> {
        match file_write_at(self.file, offset, buf) {
            size if size >= 0 => Ok(size as usize),
            e => panic!("write_at {}", e),
        }
    }

    fn set_len(&self, len: usize) -> DevResult<()> {
        match file_set_len(self.file, len) {
            0 => Ok(()),
            e => panic!("set_len {}", e),
        }
    }

    fn flush(&self) -> DevResult<()> {
        match file_flush(self.file) {
            0 => Ok(()),
            e => panic!("flush {}", e),
        }
    }
  
    fn get_file_mac(&self) -> DevResult<SefsMac> {

        let mut mac: sgx_aes_gcm_128bit_tag_t = [0u8;16];

        file_get_mac(self.file, &mut mac);        
        let sefs_mac = SefsMac(mac);
        Ok(sefs_mac)
  }
}

impl Drop for SgxFile {
    fn drop(&mut self) {
        let _ = file_close(self.file);
    }
}

/// Ecall functions to access SgxFile
extern {
    fn ecall_file_open(eid: sgx_enclave_id_t, retval: *mut size_t, path: *const u8, create: uint8_t, integrity_only: i32) -> sgx_status_t;
    fn ecall_file_close(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t) -> sgx_status_t;
    fn ecall_file_flush(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t) -> sgx_status_t;
    fn ecall_file_read_at(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, offset: size_t, buf: *mut uint8_t, len: size_t) -> sgx_status_t;
    fn ecall_file_write_at(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, offset: size_t, buf: *const uint8_t, len: size_t) -> sgx_status_t;
    fn ecall_file_set_len(eid: sgx_enclave_id_t, retval: *mut i32, fd: size_t, len: size_t) -> sgx_status_t;
    fn ecall_file_get_mac(eid: sgx_enclave_id_t, retvat: *mut i32, fd: size_t, mac: *mut uint8_t, len: size_t) -> sgx_status_t;
}

/// Must be set when init enclave
static mut EID: sgx_enclave_id_t = 0;

fn file_get_mac(fd: usize, mac: *mut sgx_aes_gcm_128bit_tag_t) -> usize {

    let mut ret_val = 0;
    unsafe {
        let len = mem::size_of::<sgx_aes_gcm_128bit_tag_t>();
        let ret = ecall_file_get_mac(EID, &mut ret_val, fd,  mac as *mut u8, len);
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
    }
    ret_val as usize
}

fn file_open(path: &str, create: bool, integrity_only: bool) -> usize {
    let cpath = format!("{}\0", path);
    let mut ret_val = 0;
    unsafe {
        let ret = ecall_file_open(EID, &mut ret_val, cpath.as_ptr(), create as uint8_t, integrity_only as i32);
        assert_eq!(ret, sgx_status_t::SGX_SUCCESS);
        assert_ne!(ret_val, 0);
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
