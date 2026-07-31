mod opt;
mod process;

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::IsTerminal;
use std::path::PathBuf;
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
use crate::process::ExitStatus;

#[derive(Debug)]
pub struct Transcoder {
    opt: Opt,
    input: Format,
    output: Format,
}

fn main() {
    let status = match opt::parse() {
        Ok(tc) => run(tc).unwrap_or_else(|err| {
            eprintln!("aq: error: {err:#}");
            ExitStatus::Error
        }),
        Err(err) => {
            eprintln!(
                "aq: {err:#}\n\
                 \nUse aq --help for help with aq's command-line options\
                 \nUse jq --help for help with jq's command-line options"
            );
            ExitStatus::Error
        }
    };
    crate::process::exit(status)
}

fn run(tc: Transcoder) -> Result<ExitStatus> {
    let tc = Arc::new(tc);

    let mut cmd = Command::new("jq");

    if !tc.opt.info.filter && io::stdin().is_terminal() {
        opt::usage(tc.opt.prog.as_deref(), ExitStatus::Error);
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

    let mut status = ExitStatus::Success;

    match tc.feed_output(jq.stdout.take().expect("piped")) {
        Ok(()) => {}
        Err(err) if is_io_broken_pipe(&err) => status = ExitStatus::BrokenPipe,
        Err(err) => return Err(err),
    }

    // Now wait for `jq` to exit
    let jq_status = jq.wait().context("failed to wait for jq")?;

    // Exit with the same exit code as `jq`
    if !jq_status.success() {
        status = ExitStatus::Jq(jq_status);
    }
    // Wait for the input thread to finish and check for errors
    if let Some(rx) = rx {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Err(RecvTimeoutError::Timeout) => {
                eprintln!("aq: warn: input thread did not finish");
            }
            Err(RecvTimeoutError::Disconnected) => {
                eprintln!("aq: warn: input thread panicked");
                status = ExitStatus::Error;
            }
            Ok(exit_status) => {
                status = status.or(exit_status);
            }
        }
    }

    Ok(status)
}

impl Transcoder {
    fn feed_input(&self, mut jq: ChildStdin) -> ExitStatus {
        let mut code = ExitStatus::Success;
        let jq = &mut jq;
        if self.opt.files.is_empty() {
            if let Err(err) = self.transcode_input(io::stdin(), jq) {
                eprintln!("aq: error: {err:#}");
                code = ExitStatus::Error;
            }
        } else {
            for path in &self.opt.files {
                if let Err(err) = self.feed_input_from_path(path, jq) {
                    eprintln!("aq: error: {err:#}");
                    code = ExitStatus::Error;
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
                // Even if we wanted to, we can't transcode to TOML because of
                // the following: https://github.com/toml-rs/toml/issues/1015
                let buf = read_to_buf(jq)?;
                if buf.trim_ascii().is_empty() {
                    output.write_all(b"\n")?;
                    return Ok(());
                }

                let jv = json::from_slice(&buf).context("failed to deserialize JSON")?;
                let s = match jv {
                    json::Value::Null => String::from('\n'),
                    json::Value::Object(_) => {
                        toml::to_string(&jv).context("failed to serialize TOML")?
                    }
                    _ => {
                        let mut s = String::new();
                        let ser = toml::ser::ValueSerializer::new(&mut s);
                        serde::Serialize::serialize(&jv, ser)
                            .context("failed to serialize TOML")?;
                        if !s.ends_with('\n') {
                            s.push('\n');
                        }
                        s
                    }
                };
                output.write_all(s.as_bytes())?;
            }
            Format::Yaml => {
                let buf = read_to_buf(jq)?;
                if buf.trim_ascii().is_empty() {
                    output.write_all(b"\n")?;
                    return Ok(());
                }

                let mut de = json::Deserializer::from_slice(&buf);
                let mut ser = yaml::Serializer::new(output);
                transcode(&mut de, &mut ser).context("failed to convert from JSON to YAML")?;
            }
        }
        Ok(())
    }
}

fn read_to_buf<R: Read>(mut input: R) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    input.read_to_end(&mut buf)?;
    Ok(buf)
}

fn is_io_broken_pipe(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        // TODO: This is a workaround for the fact that yaml_serde::Error does
        // not correctly implement .source() for IO errors.
        // See https://github.com/yaml/yaml-serde/pull/11
        if let Some(error) = cause.downcast_ref::<yaml::Error>() {
            if error.to_string().starts_with("Broken pipe (os error") {
                return true;
            }
        } else if let Some(io_error) = cause.downcast_ref::<io::Error>() {
            return io_error.kind() == io::ErrorKind::BrokenPipe;
        }
    }
    false
}
