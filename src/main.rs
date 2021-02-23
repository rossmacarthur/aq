use std::ffi::OsString;
use std::io;
use std::io::prelude::*;
use std::process::{Command, Stdio};

use clap::{AppSettings, Clap};

#[derive(Clap)]
#[clap(
    global_setting = AppSettings::DeriveDisplayOrder,
    global_setting = AppSettings::DisableHelpSubcommand,
    global_setting = AppSettings::GlobalVersion,
    global_setting = AppSettings::TrailingVarArg,
)]
struct Opt {
    /// Arguments to be passed directly to jq
    #[clap()]
    args: Vec<OsString>,
}

fn main() -> io::Result<()> {
    let opt = Opt::parse();
    let mut child = Command::new("jq")
        .args(&opt.args)
        .stdin(Stdio::piped())
        .spawn()?;
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    {
        let stdin = child.stdin.take().unwrap();
        let mut deserializer = toml::Deserializer::new(&buffer);
        let mut serializer = serde_json::Serializer::new(stdin);
        serde_transcode::transcode(&mut deserializer, &mut serializer)?;
    }
    child.wait_with_output()?;
    Ok(())
}
