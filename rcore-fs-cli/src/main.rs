#![deny(warnings)]

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use structopt::StructOpt;

use rcore_fs::dev::std_impl::StdTimeProvider;
use rcore_fs::vfs::FileSystem;
#[cfg(feature = "use_fuse")]
use rcore_fs_cli::fuse::VfsFuse;
use rcore_fs_cli::zip::{unzip_dir, zip_dir};
use rcore_fs_ramfs as ramfs;
use rcore_fs_sefs as sefs;
use rcore_fs_sefs::dev::std_impl::StdUuidProvider;
use rcore_fs_sfs as sfs;

use git_version::git_version;

#[derive(Debug, StructOpt)]
#[structopt(about = "Command line tools to manage rCore file systems.")]
enum Opt {
    /// Create a new fs image from given directory.
    #[structopt(name = "zip")]
    Zip {
        /// Source directory
        #[structopt(parse(from_os_str))]
        dir: PathBuf,

        /// Image file
        #[structopt(parse(from_os_str))]
        image: PathBuf,

        /// File system: [sfs | sefs]
        #[structopt(short = "f", long = "fs", default_value = "sfs")]
        fs: String,
    },

    /// Extract files from a fs image.
    #[structopt(name = "unzip")]
    Unzip {
        /// Image file
        #[structopt(parse(from_os_str))]
        image: PathBuf,

        /// Target directory
        #[structopt(parse(from_os_str))]
        dir: PathBuf,

        /// File system: [sfs | sefs]
        #[structopt(short = "f", long = "fs", default_value = "sfs")]
        fs: String,
    },

    /// Mount a fs image to host.
    #[cfg(feature = "use_fuse")]
    #[structopt(name = "mount")]
    Mount {
        /// Image file
        #[structopt(parse(from_os_str))]
        image: PathBuf,

        /// Mount point directory
        #[structopt(parse(from_os_str))]
        mount_point: PathBuf,

        /// File system: [sfs | sefs]
        #[structopt(short = "f", long = "fs", default_value = "sfs")]
        fs: String,
    },

    #[structopt(name = "git-version")]
    GitVersion,
}

fn main() {
    env_logger::init();
    let opt = Opt::from_args();

    match opt {
        Opt::Zip { dir, image, fs } => {
            let fs = open_fs(&fs, &image, true);
            zip_dir(&dir, fs.root_inode()).expect("failed to zip fs");
        }
        Opt::Unzip { dir, image, fs } => {
            let fs = open_fs(&fs, &image, false);
            std::fs::create_dir(&dir).expect("failed to create dir");
            unzip_dir(&dir, fs.root_inode()).expect("failed to unzip fs");
        }
        #[cfg(feature = "use_fuse")]
        Opt::Mount {
            image,
            mount_point,
            fs,
        } => {
            let fs = open_fs(&fs, &image, !image.exists());
            fuse::mount(VfsFuse::new(fs), &mount_point, &[]).expect("failed to mount fs");
        }
        Opt::GitVersion => {
            println!("{}", git_version!());
        }
    }
}

/// Open or create file system image.
fn open_fs(fs: &str, image: &Path, create: bool) -> Arc<dyn FileSystem> {
    match fs {
        "sfs" => {
            let file = OpenOptions::new()
                .read(true)
                .write(create)
                .create(create)
                .truncate(create)
                .open(image)
                .expect("failed to open image");
            let device = Mutex::new(file);
            const MAX_SPACE: usize = 0x1000 * 0x1000 * 1024; // 1G
            match create {
                true => sfs::SimpleFileSystem::create(Arc::new(device), MAX_SPACE)
                    .expect("failed to create sfs"),
                false => sfs::SimpleFileSystem::open(Arc::new(device)).expect("failed to open sfs"),
            }
        }
        "sefs" => {
            std::fs::create_dir_all(image).unwrap();
            let device = sefs::dev::StdStorage::new(image);
            match create {
                true => sefs::SEFS::create(Box::new(device), &StdTimeProvider, &StdUuidProvider)
                    .expect("failed to create sefs"),
                false => sefs::SEFS::open(Box::new(device), &StdTimeProvider, &StdUuidProvider)
                    .expect("failed to open sefs"),
            }
        }
        "ramfs" => ramfs::RamFS::new(),
        _ => panic!("unsupported file system"),
    }
}
