use std::env;
use std::process;

use anyhow::{bail, Context, Result};

use crate::Transcoder;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Format {
    #[default]
    Json,
    Toml,
    Yaml,
}

impl Format {
    fn from_str(s: &str) -> Result<Self> {
        Ok(match s {
            "j" | "json" => Self::Json,
            "t" | "toml" => Self::Toml,
            "y" | "yaml" => Self::Yaml,
            _ => bail!("invalid format `{}`, expected `json`, `toml`, or `yaml`", s),
        })
    }
}

pub fn usage() {
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
    eprintln!("{USAGE}");
    process::exit(0)
}

pub fn args() -> Result<Transcoder> {
    let mut args = env::args_os().skip(1);

    let mut input: Option<Format> = None;
    let mut output: Option<Format> = None;
    let mut input_raw = false;
    let mut output_raw = false;
    let mut jq_args = Vec::with_capacity(args.len());

    while let Some(arg) = args.next() {
        let missing = || {
            format!(
                "the argument `{}` requires a value but none was supplied",
                arg.to_str().unwrap(),
            )
        };
        match arg.as_os_str().to_str() {
            Some("--") => break,
            Some("-h" | "--help") => usage(),
            Some("-i" | "--input") => {
                let fmt = args.next().with_context(missing)?;
                let fmt = fmt.to_str().context("invalid UTF-8")?;
                input = Some(Format::from_str(fmt)?);
            }
            Some(arg) if arg.starts_with("-i") => {
                let fmt = &arg[2..].trim_start_matches('=');
                input = Some(Format::from_str(fmt)?);
            }
            Some(arg) if arg.starts_with("--input=") => {
                input = Some(Format::from_str(&arg[8..])?);
            }
            Some("-o" | "--output") => {
                let fmt = args.next().with_context(missing)?;
                let fmt = fmt.to_str().context("invalid UTF-8")?;
                output = Some(Format::from_str(fmt)?);
            }
            Some(arg) if arg.starts_with("--output=") => {
                output = Some(Format::from_str(&arg[9..])?);
            }
            Some(arg) if arg.starts_with("-o") => {
                let fmt = &arg[2..].trim_start_matches('=');
                output = Some(Format::from_str(fmt)?);
            }
            Some("--raw-input") => {
                input_raw = true;
                jq_args.push(arg);
            }
            Some("--raw-output") => {
                output_raw = true;
                jq_args.push(arg);
            }
            Some(args) if args.starts_with('-') && !args.starts_with("--") => {
                if args.contains('r') {
                    output_raw = true;
                }
                if args.contains('R') {
                    input_raw = true;
                }
                jq_args.push(arg);
            }
            _ => {
                jq_args.push(arg);
            }
        }
    }

    jq_args.extend(args);

    let input = input.unwrap_or_default();
    let output = output.unwrap_or(if output_raw { Format::Json } else { input });

    if input_raw && input != Format::Json {
        bail!("`-R` is only compatible with JSON input")
    }
    if output_raw && output != Format::Json {
        bail!("`-r` is only compatible with JSON output")
    }

    Ok(Transcoder {
        input,
        output,
        jq_args,
    })
}
