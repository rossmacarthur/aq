mod parse;

use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process;
use std::process::{ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
    let tc = Arc::new(parse::args()?);

    let mut cmd = Command::new("jq");

    if !tc.opt.info.filter && io::stdin().is_terminal() {
        parse::usage();
    }

    if tc.opt.force_color_output && io::stdout().is_terminal() {
        cmd.arg("-C");
    }

    cmd.args(&tc.opt.args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut jq = cmd.spawn()?;

    // Feed input to `jq` in a separate thread
    let rx = if tc.opt.info.null_input {
        None
    } else {
        let (tx, rx) = mpsc::channel();
        let tc = tc.clone();
        let stdin = jq.stdin.take().expect("piped");
        thread::spawn(move || {
            if let Err(err) = tx.send(tc.feed_input(stdin)) {
                panic!("aq: failed to send result from thread: {}", err);
            }
        });
        Some(rx)
    };

    let stdout = jq.stdout.take().expect("piped");
    tc.feed_output(stdout)?;

    // Now wait for `jq` to exit
    let status = jq.wait()?;

    // Exit with the same exit code as `jq`
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }

    // Wait for the input thread to finish and check for errors
    if let Some(rx) = rx {
        if let Ok(result) = rx.recv_timeout(Duration::from_millis(100)) {
            result?;
        }
    }

    Ok(())
}

impl Transcoder {
    fn feed_input(&self, mut jq: ChildStdin) -> Result<()> {
        let jq = &mut jq;
        if self.opt.files.is_empty() {
            self.transcode_input(io::stdin(), jq)
        } else {
            for path in &self.opt.files {
                if path.to_str() == Some("-") {
                    self.transcode_input(io::stdin(), jq)?;
                } else {
                    let file = File::open(path).with_context(|| {
                        format!("failed to open `{}`", PathBuf::from(path).display())
                    })?;
                    self.transcode_input(file, jq)?;
                }
            }
            Ok(())
        }
    }

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

    fn feed_output(&self, mut jq: ChildStdout) -> Result<()> {
        self.transcode_output(&mut jq, io::stdout())?;
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
