mod opt;
mod process;

use std::ffi::OsStr;
use std::fmt::Display;
use std::fmt::Write as _;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::IsTerminal;
use std::panic;
use std::path::PathBuf;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::Arc;
use std::sync::LazyLock as Lazy;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use indexmap::IndexMap;
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
    panic::set_hook(Box::new(|info| {
        let mut s = String::from("\naq: panicked");
        if let Some(payload) = info.payload_as_str() {
            write!(&mut s, " with '{payload}'").ok();
        }
        if let Some(loc) = info.location() {
            write!(&mut s, " at {}:{}", loc.file(), loc.line()).ok();
        }
        s.push_str(
            "\naq: This is probably a bug, please file an issue at\n    \
               https://github.com/rossmacarthur/aq/issues",
        );
        eprintln!("{s}");
    }));

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
    cmd.stderr(Stdio::piped());

    let mut jq = cmd.spawn().context("failed to spawn jq")?;

    // Feed input to `jq` in a separate thread
    let rx = {
        let (tx, rx) = mpsc::channel();
        let tc = tc.clone();
        let stdin = jq.stdin.take().expect("stdin piped");
        thread::spawn(move || {
            let _ = tx.send(tc.feed_input(stdin));
        });
        rx
    };

    let mut status = ExitStatus::Success;

    match tc.feed_output(jq.stdout.take().expect("stdout piped")) {
        Ok(()) => {}
        Err(err) if is_io_broken_pipe(&err) => status = ExitStatus::BrokenPipe,
        Err(err) => return Err(err),
    }

    // Now wait for `jq` to exit
    let jq_status = jq.wait().context("failed to wait for jq")?;
    let jq_stderr = read_to_buf(jq.stderr.take().expect("stderr piped"))?;

    // Exit with the same exit code as `jq`
    if !jq_status.success() {
        status = ExitStatus::Jq(jq_status);
    }
    // Wait for the input thread to finish and check for errors
    match rx.recv_timeout(Duration::from_millis(100)) {
        Err(RecvTimeoutError::Timeout) => {
            // input thread did not finish, jq likely didn't read input
            // this is normal behaviour if jq errors or when using `-n`
        }
        Err(RecvTimeoutError::Disconnected) => {
            eprintln!("aq: warn: input thread panicked");
            status = ExitStatus::Error;
        }
        Ok(exit_status) => {
            status = status.or(exit_status);
        }
    }

    // Print input errors, if any, then print the jq stderr last, it's usually
    // the most important for the user
    let mut stderr = io::stderr();
    write_input_errs(&mut stderr)?;
    if !jq_stderr.is_empty() {
        writeln!(&mut stderr)?;
        stderr.write_all(&jq_stderr)?;
    }

    Ok(status)
}

static INPUT_ERRS: Lazy<Mutex<IndexMap<String, usize>>> = Lazy::new(Default::default);

fn push_input_err(s: impl Display) {
    let mut input_errs = INPUT_ERRS.lock().unwrap();
    *input_errs.entry(format!("{s}")).or_insert(0) += 1;
}

fn write_input_errs(stderr: &mut impl Write) -> Result<()> {
    let mut input_errs = INPUT_ERRS.lock().unwrap();
    let mut input_errs = input_errs.drain(..);
    for (err, count) in input_errs.by_ref().take(7) {
        write!(stderr, "aq: error: {err}")?;
        if count > 1 {
            writeln!(stderr, " (x{count})")?;
        } else {
            writeln!(stderr)?;
        }
    }
    let err_count: usize = input_errs.map(|(_, c)| c).sum();
    if err_count > 0 {
        writeln!(
            stderr,
            "aq: error: ... and {} more input related errors",
            err_count
        )?;
    }
    Ok(())
}

impl Transcoder {
    fn feed_input(&self, mut jq: ChildStdin) -> ExitStatus {
        let mut code = ExitStatus::Success;
        let jq = &mut jq;
        if self.opt.files.is_empty() {
            if let Err(err) = self.transcode_input(io::stdin(), jq) {
                push_input_err(err);
                code = ExitStatus::Error;
            }
        } else {
            for path in &self.opt.files {
                if let Err(err) = self.feed_input_from_path(path, jq) {
                    push_input_err(err);
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
                let s = io::read_to_string(input)?;
                let de = toml::Deserializer::parse(&s)?;
                let mut ser = json::Serializer::new(jq);
                transcode(de, &mut ser).context("failed to convert from TOML to JSON")?;
            }
            Format::Yaml => {
                let de = yaml::Deserializer::from_reader(input);
                let mut ser = json::Serializer::new(jq);
                for doc in de {
                    transcode(doc, &mut ser).context("failed to convert from YAML to JSON")?
                }
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

                let jvs: Vec<json::Value> = json::Deserializer::from_slice(&buf)
                    .into_iter()
                    .collect::<json::Result<_>>()
                    .context("failed to deserialize JSON")?;

                for jv in jvs {
                    let s = json_to_toml(&jv)?;
                    output.write_all(s.as_bytes())?;
                }
            }
            Format::Yaml => {
                let buf = read_to_buf(jq)?;
                if buf.trim_ascii().is_empty() {
                    output.write_all(b"\n")?;
                    return Ok(());
                }

                let jvs: Vec<json::Value> = json::Deserializer::from_slice(&buf)
                    .into_iter()
                    .collect::<json::Result<_>>()
                    .context("failed to deserialize JSON")?;
                let sep = jvs.len() > 1;

                for jv in jvs {
                    if sep {
                        output.write_all(b"---\n")?;
                    }
                    let mut ser = yaml::Serializer::new(&mut output);
                    serde::Serialize::serialize(&jv, &mut ser)
                        .context("failed to serialize YAML")?;
                }
            }
        }
        Ok(())
    }
}

fn json_to_toml(jv: &json::Value) -> Result<String> {
    Ok(match jv {
        json::Value::Null => String::from('\n'),
        json::Value::Object(_) => toml::to_string(jv).context("failed to serialize TOML")?,
        _ => {
            let mut s = String::new();
            let ser = toml::ser::ValueSerializer::new(&mut s);
            serde::Serialize::serialize(jv, ser).context("failed to serialize TOML")?;
            if !s.ends_with('\n') {
                s.push('\n');
            }
            s
        }
    })
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
