mod parse;

use std::ffi::OsString;
use std::io;
use std::io::prelude::*;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use anyhow::bail;
use anyhow::{Context, Result};
use serde_json as json;
use serde_transcode::transcode;
use serde_yaml as yaml;

use crate::parse::Format;

#[derive(Debug)]
pub struct Transcoder {
    input: Format,
    output: Format,
    jq_args: Vec<OsString>,
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

fn main() -> Result<()> {
    let t = parse::args()?;

    let mut cmd = Command::new("jq");
    if atty::is(atty::Stream::Stdin) {
        if t.jq_args.is_empty() {
            parse::usage()
        } else {
            bail!("aq requires input via stdin");
        }
    }
    if atty::is(atty::Stream::Stdout) {
        // `jq` will detect that its stdout is a pipe so we force it to colorize
        // the output here. A user can still pass `-M` to undo this.
        if let Format::Json = t.output {
            cmd.arg("-C");
        }
    }
    cmd.args(&t.jq_args);
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
