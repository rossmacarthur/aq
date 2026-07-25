mod opt;

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use serde_json as json;
use serde_transcode::transcode;
use yaml_serde as yaml;

use crate::opt::Format;
use crate::opt::Opt;

#[derive(Debug)]
pub struct Transcoder {
    opt: Opt,
    input: Format,
    output: Format,
}

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
enum ExitCode {
    Success,
    Error,
    Jq(i32),
}

fn main() {
    let code = match opt::parse() {
        Ok(tc) => run(tc).unwrap_or_else(|err| {
            eprintln!("aq: error: {err:#}");
            ExitCode::Error
        }),
        Err(err) => {
            eprintln!(
                "aq: {err:#}\n\
                 \nUse aq --help for help with aq's command-line options\
                 \nUse jq --help for help with jq's command-line options"
            );
            ExitCode::Error
        }
    };
    process::exit(code.into());
}

fn run(tc: Transcoder) -> Result<ExitCode> {
    let tc = Arc::new(tc);

    let mut cmd = Command::new("jq");

    if !tc.opt.info.filter && io::stdin().is_terminal() {
        opt::usage(tc.opt.prog.as_deref(), ExitCode::Error);
    }

    if tc.opt.force_color_output && io::stdout().is_terminal() {
        cmd.arg("-C");
    }

    cmd.args(&tc.opt.args);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut jq = cmd.spawn().context("failed to spawn jq")?;

    // Feed input to `jq` in a separate thread
    let rx = {
        let (tx, rx) = mpsc::channel();
        let tc = tc.clone();
        let stdin = jq.stdin.take().expect("piped");
        thread::spawn(move || {
            let _ = tx.send(tc.feed_input(stdin));
        });
        Some(rx)
    };

    let stdout = jq.stdout.take().expect("piped");
    tc.feed_output(stdout)?;

    // Now wait for `jq` to exit
    let status = jq.wait().context("failed to wait for jq")?;

    let mut code = ExitCode::Success;
    // Exit with the same exit code as `jq`
    if !status.success() {
        code = status.code().map(ExitCode::Jq).unwrap_or(ExitCode::Error);
    }
    // Wait for the input thread to finish and check for errors
    if let Some(rx) = rx {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Err(RecvTimeoutError::Timeout) => {
                eprintln!("aq: warn: input thread did not finish");
            }
            Err(RecvTimeoutError::Disconnected) => {
                eprintln!("aq: warn: input thread panicked");
                code = ExitCode::Error;
            }
            Ok(exit_code) => match exit_code {
                ExitCode::Error => code = ExitCode::Error,
                ExitCode::Success => {}
                ExitCode::Jq(_) => {
                    unreachable!()
                }
            },
        }
    }

    Ok(code)
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        match code {
            ExitCode::Success => 0,
            ExitCode::Error => 2,
            ExitCode::Jq(code) => code,
        }
    }
}

impl Transcoder {
    fn feed_input(&self, mut jq: ChildStdin) -> ExitCode {
        let mut code = ExitCode::Success;
        let jq = &mut jq;
        if self.opt.files.is_empty() {
            if let Err(err) = self.transcode_input(io::stdin(), jq) {
                eprintln!("aq: error: {err:#}");
                code = ExitCode::Error;
            }
        } else {
            for path in &self.opt.files {
                if let Err(err) = self.feed_input_from_path(path, jq) {
                    eprintln!("aq: error: {err:#}");
                    code = ExitCode::Error;
                }
            }
        }
        code
    }

    fn feed_input_from_path(&self, path: &OsStr, jq: &mut ChildStdin) -> Result<()> {
        if path.to_str() == Some("-") {
            self.transcode_input(io::stdin(), jq)
        } else {
            if path.to_str() == Some("-") {
                self.transcode_input(io::stdin(), jq)
            } else {
                let file = File::open(path).with_context(|| {
                    format!("failed to open file {}", PathBuf::from(path).display())
                })?;
                self.transcode_input(file, jq)
            }
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
                transcode(de, &mut ser).context("failed to convert from TOML to JSON")?;
            }
            Format::Yaml => {
                let de = yaml::Deserializer::from_reader(input);
                let mut ser = json::Serializer::new(jq);
                transcode(de, &mut ser).context("failed to convert from YAML to JSON")?
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
                    json::from_reader(jq).context("failed to convert from JSON to TOML")?;
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
                transcode(&mut de, &mut ser).context("failed to convert from JSON to YAML")?;
            }
        }
        Ok(())
    }
}
