use crate::api::process::ExitCode;
use crate::sys;

pub fn main(_args: &[&str]) -> Result<(), ExitCode> {
    print!("{}", &sys::process::dir());
    Ok(())
}
