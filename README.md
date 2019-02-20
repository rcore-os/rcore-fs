# rcore-fs

[![Build Status](https://travis-ci.org/wangrunji0408/rcore-fs.svg?branch=master)](https://travis-ci.org/wangrunji0408/rcore-fs)

The file system module for [rCore OS](https://github.com/wangrunji0408/RustOS).

## Sub-projects

Core:

* `rcore-fs`: Interfaces and utilities that can be used in an OS.
  * Virtual File System: `FileSystem`, `INode`
  * Device and cache layer: `BlockDevice`, `CachedDevice`(TODO)
  * Unit tests for VFS

Specific file systems:

* `rcore-fs-sfs`: Simple File System from [uCore OS](https://github.com/chyyuu/ucore_os_lab)
* `rcore-fs-sefs`: Simple Encrypt File System 
* `rcore-fs-fat32`: (TODO) FAT32
* `rcore-fs-ramfs`: (TODO) RAM based FS.
* `rcore-fs-devfs`: (TODO) Device driver as files

Utilities:

* `rcore-fs-fuse`: FUSE wrapper for VFS. Mount any FS to your Linux / macOS.
* `rcore-fs-ucore`: uCore VFS wrapper for Rust VFS. Use any FS in the origin uCore. See [uCore with Rust SFS](https://github.com/wangrunji0408/ucore_os_lab/tree/rust-fs/labcodes_answer/lab8_result) for example.