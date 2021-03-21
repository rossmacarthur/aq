mod format;

use std::env;
use std::ffi::OsString;
use std::io;
use std::io::prelude::*;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};
use clap::{AppSettings, Clap};
use serde_json as json;
use serde_transcode::transcode;
use serde_yaml as yaml;

use crate::format::Format;

struct Transcoder {
    input: Format,
    output: Format,
}

impl Transcoder {
    fn from_opt(opt: &Opt) -> Self {
        let input = opt.input.unwrap_or_default();
        let output = opt.output.unwrap_or(input);
        Self { input, output }
    }

    fn transcode_input(&self, mut input: io::Stdin, jq: &mut ChildStdin) -> Result<()> {
        match self.input {
            Format::Json => {
                io::copy(&mut input, jq)?;
            }
            Format::Toml => {
                // `toml` crate only deserializes from a string :(
                let mut s = String::new();
                input.read_to_string(&mut s)?;
                let mut de = toml::Deserializer::new(&s);
                let mut ser = json::Serializer::new(jq);
                transcode(&mut de, &mut ser).context("failed to transcode from TOML to JSON")?;
            }
            Format::Yaml => {
                let de = yaml::Deserializer::from_reader(input);
                let mut ser = json::Serializer::new(jq);
                transcode(de, &mut ser).context("failed to transcode from YAML to JSON")?
            }
        }
        Ok(())
    }

    fn transcode_output(&self, jq: &mut ChildStdout, mut output: io::Stdout) -> Result<()> {
        match self.output {
            Format::Json => {
                io::copy(jq, &mut output)?;
            }
            Format::Toml => {
                // `toml` crate only serializes to a string :(
                let mut s = String::new();
                let mut de = json::Deserializer::from_reader(jq);
                let mut ser = toml::Serializer::new(&mut s);
                transcode(&mut de, &mut ser).context("failed to transcode from JSON to TOML")?;
                if !s.ends_with('\n') {
                    s.push('\n');
                }
                output.write_all(s.as_bytes())?;
            }
            Format::Yaml => {
                let mut de = json::Deserializer::from_reader(jq);
                let mut ser = yaml::Serializer::new(output);
                transcode(&mut de, &mut ser).context("failed to transcode from JSON to YAML")?;
            }
        }
        Ok(())
    }
}

#[derive(Clap)]
#[clap(
    author,
    about,
    global_setting = AppSettings::DeriveDisplayOrder,
    global_setting = AppSettings::DisableHelpSubcommand,
    global_setting = AppSettings::GlobalVersion,
)]
struct Opt {
    /// The input data format.
    #[clap(long, short, value_name = "format")]
    input: Option<Format>,

    /// The output data format [default: <input>].
    #[clap(long, short, value_name = "format")]
    output: Option<Format>,

    /// The jq filter to apply to the input.
    #[clap()]
    filter: OsString,
}

impl Opt {
    fn from_args() -> (Self, Vec<OsString>) {
        let args: Vec<_> = env::args_os().collect();
        let mut it = args.splitn(2, |a| a == "--");
        let args = it.next().unwrap();
        let jq_args = it.next().unwrap_or_default();
        let opt = Opt::parse_from(args);
        (opt, jq_args.to_vec())
    }
}

fn main() -> Result<()> {
    let (opt, jq_args) = Opt::from_args();
    let t = Transcoder::from_opt(&opt);

    let mut cmd = Command::new("jq");
    if atty::is(atty::Stream::Stdout) {
        // `jq` will detect that its stdout is a pipe so we force it to colorize
        // the output here.
        if let Format::Json = t.output {
            cmd.arg("-C");
        }
    }
    cmd.arg(&opt.filter);
    cmd.args(&jq_args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    // Spawn `jq` and transcode input and output
    let mut jq = cmd.spawn()?;

    // NB! `stdin` must be dropped otherwise `jq` will never exit
    {
        let mut stdin = jq.stdin.take().unwrap();
        t.transcode_input(io::stdin(), &mut stdin)?;
    }
    let mut stdout = jq.stdout.take().unwrap();
    t.transcode_output(&mut stdout, io::stdout())?;

    jq.wait()?;
    Ok(())
}
