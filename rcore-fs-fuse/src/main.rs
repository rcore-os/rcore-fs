use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use structopt::StructOpt;

use rcore_fs::dev::std_impl::StdTimeProvider;
use rcore_fs::vfs::FileSystem;
#[cfg(feature = "use_fuse")]
use rcore_fs_fuse::fuse::VfsFuse;
use rcore_fs_fuse::zip::{unzip_dir, zip_dir};
use rcore_fs_sefs as sefs;
use rcore_fs_sfs as sfs;

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

    /// File system: [sfs | sefs]
    #[structopt(short = "f", long = "fs", default_value = "sfs")]
    fs: String,
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
    #[cfg(feature = "use_fuse")]
    #[structopt(name = "mount")]
    Mount,
}

fn main() {
    env_logger::init().unwrap();
    let opt = Opt::from_args();

    // open or create
    let create = match opt.cmd {
        #[cfg(feature = "use_fuse")]
        Cmd::Mount => !opt.image.is_dir() && !opt.image.is_file(),
        Cmd::Zip => true,
        Cmd::Unzip => false,
    };

    let fs: Arc<FileSystem> = match opt.fs.as_str() {
        "sfs" => {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&opt.image)
                .expect("failed to open image");
            let device = Mutex::new(file);
            const MAX_SPACE: usize = 0x1000 * 0x1000 * 1024; // 1G
            match create {
                true => sfs::SimpleFileSystem::create(Arc::new(device), MAX_SPACE),
                false => sfs::SimpleFileSystem::open(Arc::new(device)).expect("failed to open sfs"),
            }
        }
        "sefs" => {
            std::fs::create_dir_all(&opt.image).unwrap();
            let device = sefs::dev::StdStorage::new(&opt.image);
            match create {
                true => sefs::SEFS::create(Box::new(device), &StdTimeProvider)
                    .expect("failed to create sefs"),
                false => sefs::SEFS::open(Box::new(device), &StdTimeProvider)
                    .expect("failed to open sefs"),
            }
        }
        _ => panic!("unsupported file system"),
    };
    match opt.cmd {
        #[cfg(feature = "use_fuse")]
        Cmd::Mount => {
            fuse::mount(VfsFuse::new(fs), &opt.dir, &[]).expect("failed to mount fs");
        }
        Cmd::Zip => {
            zip_dir(&opt.dir, fs.root_inode()).expect("failed to zip fs");
        }
        Cmd::Unzip => {
            std::fs::create_dir(&opt.dir).expect("failed to create dir");
            unzip_dir(&opt.dir, fs.root_inode()).expect("failed to unzip fs");
        }
    }
}
