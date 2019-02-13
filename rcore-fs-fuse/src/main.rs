use std::fs::OpenOptions;
use std::path::PathBuf;

use structopt::StructOpt;

use rcore_fs_sefs as sefs;
use rcore_fs_sfs as sfs;
use rcore_fs_fuse::VfsFuse;

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
//    let img = OpenOptions::new().read(true).write(true).open(&opt.image)
//        .expect("failed to open image");
    let sfs = if opt.image.is_dir() {
        let img = sefs::dev::StdStorage::new(&opt.image);
        sefs::SEFS::open(Box::new(img))
            .expect("failed to open sefs")
    } else {
        std::fs::create_dir_all(&opt.image).unwrap();
        let img = sefs::dev::StdStorage::new(&opt.image);
        sefs::SEFS::create(Box::new(img))
            .expect("failed to create sefs")
    };
    fuse::mount(VfsFuse::new(sfs), &opt.mount_point, &[])
        .expect("failed to mount fs");
}
