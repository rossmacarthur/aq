mod format;

use std::ffi::OsString;
use std::io;
use std::io::prelude::*;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::str;

use clap::{AppSettings, Clap};
use serde_json as json;
use serde_transcode::transcode;

use crate::format::Format;
use anyhow::Result;

struct Transcoder {
    input: Format,
    output: Format,
}

impl Transcoder {
    fn transcode_input(&self, input: Vec<u8>, jq: &mut ChildStdin) -> Result<()> {
        match self.input {
            Format::Json => jq.write_all(&input)?,
            Format::Toml => {
                let input = String::from_utf8(input)?;
                let mut de = toml::Deserializer::new(&input);
                let mut ser = json::Serializer::new(jq);
                transcode(&mut de, &mut ser)?;
            }
        }
        Ok(())
    }

    fn transcode_output(&self, jq: &mut ChildStdout, mut output: io::Stdout) -> Result<()> {
        match self.output {
            Format::Json => {
                let mut buf = Vec::new();
                jq.read_to_end(&mut buf)?;
                output.write_all(&buf)?;
            }
            Format::Toml => {
                let mut de = json::Deserializer::from_reader(jq);
                let mut buf = String::new();
                let mut ser = toml::Serializer::new(&mut buf);
                transcode(&mut de, &mut ser)?;
                if !buf.ends_with('\n') {
                    buf.push('\n');
                }
                output.write_all(buf.as_bytes())?;
            }
        }
        Ok(())
    }
}

#[derive(Clap)]
#[clap(
    global_setting = AppSettings::DeriveDisplayOrder,
    global_setting = AppSettings::DisableHelpSubcommand,
    global_setting = AppSettings::GlobalVersion,
    global_setting = AppSettings::TrailingVarArg,
)]
struct Opt {
    /// The input data format.
    #[clap(short, default_value, value_name = "format")]
    input: Format,

    /// The output data format [default: <input>].
    #[clap(short, value_name = "format")]
    output: Option<Format>,

    /// Arguments to be passed directly to jq...
    #[clap()]
    args: Vec<OsString>,
}

fn main() -> Result<()> {
    let Opt {
        input,
        output,
        args,
    } = Opt::parse();
    let output = output.unwrap_or(input);
    let t = Transcoder { input, output };

    let mut cmd = Command::new("jq");
    cmd.args(&args);

    if atty::isnt(atty::Stream::Stdin) {
        // Read in the entire input
        let mut input = Vec::new();
        io::stdin().read_to_end(&mut input)?;

        // Setup pipes
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());

        // Spawn jq and get handles to `stdin` and `stdout`
        let mut jq = cmd.spawn()?;
        let mut stdin = jq.stdin.take().unwrap();
        let mut stdout = jq.stdout.take().unwrap();

        t.transcode_input(input, &mut stdin)?;

        // NB! Otherwise `jq` will never exit
        drop(stdin);

        t.transcode_output(&mut stdout, io::stdout())?;

        jq
    } else {
        // There is no stdin, so just spawn `jq` with the given args
        cmd.spawn()?
    }
    .wait()?;
    Ok(())
}
