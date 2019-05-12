#![cfg_attr(not(any(test, feature = "std")), no_std)]
#![feature(alloc)]

extern crate alloc;

extern crate ext2;

#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use core::ops::Range;
use ext2::error::Error;
use ext2::fs::sync::Synced;
use ext2::fs::Ext2;
use ext2::sector::{Address, Size512};
use ext2::volume::size::Size;
use ext2::volume::{Volume, VolumeCommit, VolumeSlice};
use rcore_fs::dev::{DevError, Device};
use rcore_fs::vfs;

#[derive(Clone)]
struct Ext2Volume {
    inner: Arc<Device>,
}

#[derive(Clone)]
pub struct Ext2FileSystem {
    inner: Synced<Ext2<Size512, Ext2Volume>>,
    volume: Ext2Volume,
}

/// A conversion between vfs::FsError and ext2::Error
#[derive(Debug)]
struct Ext2Error {
    inner: Error,
}

impl core::convert::From<Ext2Error> for vfs::FsError {
    fn from(err: Ext2Error) -> Self {
        match err.inner {
            _ => vfs::FsError::DeviceError,
        }
    }
}

impl core::convert::From<Ext2Error> for Error {
    fn from(err: Ext2Error) -> Self {
        err.inner
    }
}

impl core::convert::From<Error> for Ext2Error {
    fn from(err: Error) -> Self {
        Ext2Error { inner: err }
    }
}

impl core::convert::From<DevError> for Ext2Error {
    fn from(_: DevError) -> Self {
        Ext2Error {
            inner: Error::Other(String::from("unknown")),
        }
    }
}

impl Ext2FileSystem {
    pub fn open(device: Arc<Device>) -> vfs::Result<Arc<Self>> {
        Ok(Self::open_internal(device)?)
    }

    fn open_internal(device: Arc<Device>) -> Result<Arc<Self>, Ext2Error> {
        let volume = Ext2Volume { inner: device };
        let fs = Synced::new(volume.clone())?;
        Ok(Arc::new(Ext2FileSystem { inner: fs, volume }))
    }
}

impl Volume<u8, Size512> for Ext2Volume {
    type Error = Ext2Error;

    fn size(&self) -> Size<Size512> {
        Size::Unbounded
    }

    fn commit(&mut self, _slice: Option<VolumeCommit<u8, Size512>>) -> Result<(), Self::Error> {
        unimplemented!()
    }

    unsafe fn slice_unchecked<'a>(
        &'a self,
        range: Range<Address<Size512>>,
    ) -> VolumeSlice<'a, u8, Size512> {
        let index = range.start;
        let len = range.end - range.start;
        let mut vec = vec![0; len.into_index() as usize];
        self.inner
            .read_at(index.into_index() as usize, vec.as_mut_slice())
            .unwrap();
        VolumeSlice::new_owned(vec, index)
    }

    fn slice<'a>(
        &'a self,
        range: Range<Address<Size512>>,
    ) -> Result<VolumeSlice<'a, u8, Size512>, Self::Error> {
        let index = range.start;
        let len = range.end - range.start;
        let mut vec = vec![0; len.into_index() as usize];
        self.inner
            .read_at(index.into_index() as usize, vec.as_mut_slice())?;
        Ok(VolumeSlice::new_owned(vec, index))
    }
}
