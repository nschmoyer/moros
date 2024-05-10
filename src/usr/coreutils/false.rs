use crate::api::process::ExitCode;

pub fn main(_args: &[&str]) -> Result<(), ExitCode> {
    Err(ExitCode::Failure)
}
