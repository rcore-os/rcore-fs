use alloc::boxed::Box;
use alloc::prelude::{String, ToString};
use core::fmt::{Debug, Error, Formatter};
use rcore_fs::vfs::FsError;

#[cfg(any(test, feature = "std"))]
pub use self::std_impl::*;
pub mod std_impl;

/// A file stores a normal file or directory.
///
/// The interface is same as `std::fs::File`.
pub trait File: Send + Sync {
    fn read_at(&self, buf: &mut [u8], offset: usize) -> DevResult<usize>;
    fn write_at(&self, buf: &[u8], offset: usize) -> DevResult<usize>;
    fn set_len(&self, len: usize) -> DevResult<()>;
    fn flush(&self) -> DevResult<()>;
    fn get_file_mac(&self) -> DevResult<SefsMac>;

    fn read_exact_at(&self, buf: &mut [u8], offset: usize) -> DevResult<()> {
        let len = self.read_at(buf, offset)?;
        if len == buf.len() {
            Ok(())
        } else {
            Err(DeviceError)
        }
    }
    fn write_all_at(&self, buf: &[u8], offset: usize) -> DevResult<()> {
        let len = self.write_at(buf, offset)?;
        if len == buf.len() {
            Ok(())
        } else {
            Err(DeviceError)
        }
    }
}

/// The collection of all files in the FS.
pub trait Storage: Send + Sync {
    fn open(&self, file_id: &str) -> DevResult<Box<dyn File>>;
    fn create(&self, file_id: &str) -> DevResult<Box<dyn File>>;
    fn remove(&self, file_id: &str) -> DevResult<()>;
    fn is_integrity_only(&self) -> bool {
        false
    }
}

#[repr(C)]
pub struct SefsUuid(pub [u8; 16]);

impl alloc::string::ToString for SefsUuid {
    fn to_string(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl Debug for SefsUuid {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "SefsUuid({})", self.to_string())
    }
}

pub trait UuidProvider: Send + Sync {
    fn generate_uuid(&self) -> SefsUuid;
}

pub const SGX_AESGCM_MAC_SIZE: usize = 16;
#[allow(non_camel_case_types)]
pub type sgx_aes_gcm_128bit_tag_t = [u8; SGX_AESGCM_MAC_SIZE];

#[repr(C)]
#[derive(PartialEq, Eq, Default)]
pub struct SefsMac(pub sgx_aes_gcm_128bit_tag_t);

impl SefsMac {
    pub fn is_empty(&self) -> bool {
        *self == Default::default()
    }
}

impl alloc::string::ToString for SefsMac {
    fn to_string(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl Debug for SefsMac {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Debug)]
pub struct DeviceError;

pub type DevResult<T> = Result<T, DeviceError>;

impl From<DeviceError> for FsError {
    fn from(_: DeviceError) -> Self {
        FsError::DeviceError
    }
}
