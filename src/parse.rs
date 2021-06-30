use std::env;
use std::ffi::OsStr;
use std::process;

use anyhow::{bail, Context, Result};

use crate::Transcoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Json,
    Toml,
    Yaml,
}

impl Default for Format {
    fn default() -> Self {
        Self::Json
    }
}

impl Format {
    fn from_str(s: &str) -> Result<Self> {
        Self::from_os_str(OsStr::new(s))
    }

    fn from_os_str(s: &OsStr) -> Result<Self> {
        Ok(match s.to_str() {
            Some("j" | "json") => Self::Json,
            Some("t" | "toml") => Self::Toml,
            Some("y" | "yaml") => Self::Yaml,
            _ => bail!(
                "invalid format `{}`, expected `json`, `toml`, or `yaml`",
                s.to_string_lossy()
            ),
        })
    }
}

fn usage() {
    const USAGE: &str = r#"Usage: aq [options] <jq filter>

aq is a command line JSON / TOML / YAML processor built on top
of jq by transcoding to and from JSON.

Options:
    -i, --input <fmt>  the input data format [default: json]
    -o, --output <fmt> the output data format [default: input]

Where <fmt> is one of json, toml, or yaml. Formats can also be
specified using the shorthand j, t, or y.

Example (input JSON, output TOML):

    $ echo '{"foo": 1337}' | aq -ij -ot .
    foo = 1337

aq passes all other options and arguments directly to jq.
See jq --help or the jq man page for more options."#;
    eprintln!("{}", USAGE);
    process::exit(0)
}

pub fn args() -> Result<Transcoder> {
    let mut args = env::args_os().skip(1);

    let mut input: Option<Format> = None;
    let mut output: Option<Format> = None;
    let mut jq_args = Vec::with_capacity(args.len());

    while let Some(arg) = args.next() {
        let missing = || {
            format!(
                "the argument `{}` requires a value but none was supplied",
                arg.to_string_lossy(),
            )
        };
        match arg.as_os_str().to_str() {
            Some("--") => break,
            Some("-h" | "--help") => usage(),
            Some("-i" | "--input") => {
                let fmt = args.next().with_context(missing)?;
                input = Some(Format::from_os_str(&fmt)?);
            }
            Some(arg) if arg.starts_with("-i") => {
                input = Some(Format::from_str(&arg[2..])?);
            }
            Some("-o" | "--output") => {
                let fmt = args.next().with_context(missing)?;
                output = Some(Format::from_os_str(&fmt)?);
            }
            Some(arg) if arg.starts_with("-o") => {
                output = Some(Format::from_str(&arg[2..])?);
            }
            _ => jq_args.push(arg),
        }
    }

    jq_args.extend(args);

    let input = input.unwrap_or_default();
    let output = output.unwrap_or(input);
    Ok(Transcoder {
        input,
        output,
        jq_args,
    })
}
