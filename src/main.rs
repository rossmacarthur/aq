mod format;

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
    global_setting = AppSettings::DeriveDisplayOrder,
    global_setting = AppSettings::DisableHelpSubcommand,
    global_setting = AppSettings::GlobalVersion,
    global_setting = AppSettings::TrailingVarArg,
)]
struct Opt {
    /// The input data format.
    #[clap(short, value_name = "format")]
    input: Option<Format>,

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
    let input = input.unwrap_or_default();
    let output = output.unwrap_or(input);
    let t = Transcoder { input, output };

    let mut cmd = Command::new("jq");
    cmd.args(&args);

    if atty::isnt(atty::Stream::Stdin) {
        // Setup pipes
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());

        // Spawn jq and get handles to `stdin` and `stdout`
        let mut jq = cmd.spawn()?;
        let mut stdin = jq.stdin.take().unwrap();
        let mut stdout = jq.stdout.take().unwrap();

        t.transcode_input(io::stdin(), &mut stdin)?;

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
