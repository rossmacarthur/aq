mod parse;

use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};
use serde_json as json;
use serde_transcode::transcode;
use yaml_serde as yaml;

use crate::parse::{Format, Opt};

#[derive(Debug)]
pub struct Transcoder {
    opt: Opt,
    input: Format,
    output: Format,
}

fn main() -> Result<()> {
    let t = parse::args()?;

    let mut cmd = Command::new("jq");

    if io::stdin().is_terminal() && !t.opt.info.filter {
        parse::usage();
    }

    if io::stdout().is_terminal() {
        // `jq` will detect that its stdout is a pipe so we force it to colorize
        // the output here. A user can still pass `-M` to undo this.
        if let Format::Json = t.output {
            cmd.arg("-C");
        }
    }

    cmd.args(&t.opt.args);
    cmd.stdin(Stdio::piped());
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    // Spawn `jq` and transcode input and output
    let mut jq = cmd.spawn()?;

    // NB! `stdin` must be dropped otherwise `jq` will never exit
    {
        let mut stdin = jq.stdin.take().unwrap();
        if t.opt.files.is_empty() {
            t.transcode_input(io::stdin(), &mut stdin)?;
        } else {
            for path in &t.opt.files {
                if path.to_str() == Some("-") {
                    t.transcode_input(io::stdin(), &mut stdin)?;
                } else {
                    let file = File::open(path).with_context(|| {
                        format!("failed to open `{}`", PathBuf::from(path).display())
                    })?;
                    t.transcode_input(file, &mut stdin)?;
                }
            }
        }
    }
    let mut stdout = jq.stdout.take().unwrap();
    t.transcode_output(&mut stdout, io::stdout())?;

    jq.wait()?;
    Ok(())
}

impl Transcoder {
    fn transcode_input<R: Read>(&self, mut input: R, jq: &mut ChildStdin) -> Result<()> {
        match self.input {
            Format::Json => {
                io::copy(&mut input, jq)?;
            }
            Format::Toml => {
                // `toml` crate only deserializes from a string :(
                let mut s = String::new();
                input.read_to_string(&mut s)?;
                let de = toml::Deserializer::parse(&s)?;
                let mut ser = json::Serializer::new(jq);
                transcode(de, &mut ser).context("failed to transcode from TOML to JSON")?;
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
                // Skip transcode here because of the following
                // https://github.com/toml-rs/toml/issues/1015
                let value: json::Value =
                    json::from_reader(jq).context("failed to transcode from JSON to TOML")?;
                let s = match value {
                    json::Value::Object(_) => {
                        toml::to_string(&value).context("failed to serialize to TOML")?
                    }
                    _ => {
                        let mut s = String::new();
                        let ser = toml::ser::ValueSerializer::new(&mut s);
                        serde::Serialize::serialize(&value, ser)
                            .context("failed to serialize to TOML")?;
                        if !s.ends_with('\n') {
                            s.push('\n');
                        }
                        s
                    }
                };
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
