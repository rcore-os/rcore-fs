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

use std::path::PathBuf;

use structopt::StructOpt;

use rcore_fs_fuse::fuse::VfsFuse;
use rcore_fs_fuse::zip::{zip_dir, unzip_dir};
use rcore_fs_sefs as sefs;
use rcore_fs::dev::std_impl::StdTimeProvider;
use rcore_fs::vfs::FileSystem;

mod sgx_dev;
mod enclave;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Command
    #[structopt(subcommand)]
    cmd: Cmd,

    /// Image file
    #[structopt(parse(from_os_str))]
    image: PathBuf,

    /// Target directory
    #[structopt(parse(from_os_str))]
    dir: PathBuf,

    /// Integrity-only mode
    #[structopt(short = "i", long = "integrity-only")]
    integrity_only: bool,
}

#[derive(Debug, StructOpt)]
enum Cmd {
    /// Create a new <image> for <dir>
    #[structopt(name = "zip")]
    Zip,

    /// Unzip data from given <image> to <dir>
    #[structopt(name = "unzip")]
    Unzip,

    /// Mount <image> to <dir>
    #[structopt(name = "mount")]
    Mount,
}

fn main() {
    env_logger::init().unwrap();

    let opt = Opt::from_args();

    let enclave = match enclave::init_enclave() {
        Ok(r) => {
            println!("[+] Init Enclave Successful {}!", r.geteid());
            r
        },
        Err(x) => {
            println!("[-] Init Enclave Failed {}!", x.as_str());
            return;
        },
    };

    // open or create
    let create = match opt.cmd {
        Cmd::Mount => !opt.image.is_dir(),
        Cmd::Zip => true,
        Cmd::Unzip => false,
    };

    let device = sgx_dev::SgxStorage::new(enclave.geteid(),
        &opt.image, opt.integrity_only);
    let fs = match create {
        true => {
            std::fs::create_dir(&opt.image)
                .expect("failed to create dir for SEFS");
            sefs::SEFS::create(Box::new(device), &StdTimeProvider)
                .expect("failed to create sefs")
        }
        false => {
            sefs::SEFS::open(Box::new(device), &StdTimeProvider)
                .expect("failed to open sefs")
        }
    };
    match opt.cmd {
        Cmd::Mount => {
            fuse::mount(VfsFuse::new(fs), &opt.dir, &[])
                .expect("failed to mount fs");
        }
        Cmd::Zip => {
            zip_dir(&opt.dir, fs.root_inode())
                .expect("failed to zip fs");
        }
        Cmd::Unzip => {
            std::fs::create_dir(&opt.dir)
                .expect("failed to create dir");
            unzip_dir(&opt.dir, fs.root_inode())
                .expect("failed to unzip fs");
        }
    }
}
