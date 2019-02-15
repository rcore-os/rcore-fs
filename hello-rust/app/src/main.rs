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

use rcore_fs_fuse::VfsFuse;
use rcore_fs_sefs as sefs;

mod sgx_dev;
mod enclave;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Image file
    #[structopt(parse(from_os_str))]
    image: PathBuf,
    /// Mount point
    #[structopt(parse(from_os_str))]
    mount_point: PathBuf,
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

    let sfs = if opt.image.is_dir() {
        let img = sgx_dev::SgxStorage::new(enclave.geteid(), &opt.image);
        sefs::SEFS::open(Box::new(img))
            .expect("failed to open sefs")
    } else {
        std::fs::create_dir_all(&opt.image).unwrap();
        let img = sgx_dev::SgxStorage::new(enclave.geteid(), &opt.image);
        sefs::SEFS::create(Box::new(img))
            .expect("failed to create sefs")
    };
    fuse::mount(VfsFuse::new(sfs), &opt.mount_point, &[])
        .expect("failed to mount fs");
}
