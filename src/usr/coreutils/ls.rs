/// todo:
///     - add user/group to long format
///     - add rwx to long format

use crate::api::clock::DATE_TIME;
use crate::api::console::Style;
use crate::api::fs;
use crate::api::fs::FileInfo;
use crate::api::process::ExitCode;
use crate::api::syscall;
use crate::api::time;
use crate::sys;

use alloc::string::ToString;
use alloc::vec::Vec;

/// When showing files inline, how many spaces after the longest filename?
const INLINE_PAD: usize = 1;

pub fn main(args: &[&str]) -> Result<(), ExitCode> {
    let mut path: &str = &sys::process::dir();
    let mut sort = "name";
    let mut hide_dot_files = true;
    let mut cur_width = 0;
    let mut long_format = false;

    let n = args.len();
    for i in 1..n {
        match args[i] {
            "-a" => hide_dot_files = false,
            "-l" => long_format = true,
            _ => path = args[i],
        }
    }

    if let Some(info) = syscall::info(path) {
        if info.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                let mut files: Vec<_> = entries.iter().filter(|entry|
                    !(entry.name().starts_with('.') && hide_dot_files)
                ).collect();

                match sort {
                    "name" => files.sort_by_key(|f| f.name()),
                    _ => {
                        // We shouldn't ever reach this point since sorting parameters are
                        // hardcoded with ls
                        error!("ls: unrecognized sort option `{}'", sort);
                        return Err(ExitCode::Failure);
                    }
                }

                // get the largest filename length for when we're listing files inline
                let name_len = files.iter().fold(0, |max_len, file| {
                    let len = file.name().len();
                    core::cmp::max(max_len, len)
                });

                // get the largest filesize digit length for when we're listing long-form
                let size_len = files.iter().fold(0, |max_len, file| {
                    let len = file.size().to_string().len();
                    core::cmp::max(max_len, len)
                });

                for file in files {
                    // todo: use BUFFER_WIDTH instead of hardcoded width
                    if !long_format {
                        if cur_width + name_len + INLINE_PAD > 80 {
                            println!();
                            cur_width = 0;
                        } else {
                            cur_width = cur_width + name_len + INLINE_PAD;
                        }
                    }

                    print_file(file, name_len, size_len, long_format);
                }
                Ok(())
            } else {
                error!("ls: {}: No such file or directory", path);
                Err(ExitCode::Failure)
            }
        } else {
            // print for single file
            print_file(&info, info.name().len(), info.size().to_string().len(), long_format);
            Ok(())
        }
    } else {
        error!("ls: {}: No such file or directory", path);
        Err(ExitCode::Failure)
    }
}

fn print_file(file: &FileInfo, name_len: usize, size_len: usize, long_format: bool) {
    let csi_dir_color = Style::color("Cyan");
    let csi_reset = Style::reset();

    let color = if file.is_dir() {
        csi_dir_color
    } else {
        csi_reset
    };

    if long_format {
        let time = time::from_timestamp(file.time() as i64).format(DATE_TIME);
        print!("{:>size_len$} {} ", file.size(), time);
    }

    let len = name_len + INLINE_PAD;
    print!("{}{:<len$}{}",
           color,
           file.name(),
           csi_reset,
    );

    if long_format {
        println!();
    }
}