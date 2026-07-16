use std::env;
use std::ffi::OsString;
use std::process;

use anyhow::{bail, Context, Result};

use crate::Transcoder;

#[derive(Debug, Default)]
pub struct Opt {
    /// Info about some jq options that are set
    pub info: Info,
    /// Arguments to pass to jq
    pub args: Vec<OsString>,
    /// Files to read input from
    pub files: Vec<OsString>,
}

#[derive(Debug, Default)]
pub struct Info {
    pub filter: bool,
    pub null_input: bool,
    pub raw_input: bool,
    pub raw_output: bool,
    pub args: bool,
    pub jsonargs: bool,
}

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

pub fn usage() -> ! {
    const USAGE: &str = r#"aq - command line JSON / TOML / YAML processor
     built on top of jq by transcoding to and from JSON.

Usage: aq [options] <jq filter> [file...]

Options:
    -i, --input <fmt>  the input data format [default: json]
    -o, --output <fmt> the output data format [default: input]
    ...                other options are passed directly to jq

Where <fmt> is one of json, toml, or yaml. Formats can also be
specified using the shorthand j, t, or y.

Example (input JSON, output TOML):

    $ echo '{"foo": 1337}' | aq -ij -ot .
    foo = 1337"

See jq --help or the jq man page for more options"#;
    eprintln!("{USAGE}");
    process::exit(0)
}

pub fn args() -> Result<Transcoder> {
    let mut iter = env::args_os().skip(1);

    let mut input: Option<Format> = None;
    let mut output: Option<Format> = None;

    let mut info = Info::default();
    let mut args = Vec::new();
    let mut files = Vec::new();

    while let Some(arg) = iter.next() {
        let missing = || {
            format!(
                "the argument `{}` requires a value but none was supplied",
                arg.to_str().unwrap(),
            )
        };
        match arg.as_os_str().to_str() {
            Some("-h" | "--help") => {
                usage();
            }
            Some("--") => {
                // This signals that all remaining arguments are not options
                files.extend(iter);
                break;
            }
            Some("-i" | "--input") => {
                let fmt = iter.next().with_context(missing)?;
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
                let fmt = iter.next().with_context(missing)?;
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
            Some("--null-input") => {
                info.null_input = true;
                args.push(arg);
            }
            Some("--raw-input") => {
                info.raw_input = true;
                args.push(arg);
            }
            Some("--raw-output") => {
                info.raw_output = true;
                args.push(arg);
            }
            Some("--args") => {
                info.args = true;
                args.push(arg);
            }
            Some("--jsonargs") => {
                info.jsonargs = true;
                args.push(arg);
            }
            Some(opt) if opt != "-" && opt.starts_with('-') => {
                if !opt.starts_with("--") {
                    if opt.contains('r') {
                        info.raw_output = true;
                    }
                    if opt.contains('R') {
                        info.raw_input = true;
                    }
                    if opt.contains('n') {
                        info.null_input = true;
                    }
                }
                args.push(arg);
            }
            _ => {
                if info.filter {
                    files.push(arg);
                } else {
                    info.filter = true;
                    args.push(arg);
                }
            }
        }
    }

    if info.args || info.jsonargs {
        args.append(&mut files);
    }

    let input = input.unwrap_or_default();
    let output = output.unwrap_or(if info.raw_output { Format::Json } else { input });

    for (arg, is_set) in [
        ("-R / --raw-input", info.raw_input),
        ("--args", info.args),
        ("--jsonargs", info.jsonargs),
    ] {
        if is_set && input != Format::Json {
            bail!("`{}` is only compatible with JSON input", arg)
        }
    }
    if info.raw_output && output != Format::Json {
        bail!("`-r` is only compatible with JSON output")
    }

    Ok(Transcoder {
        input,
        output,
        opt: Opt { info, args, files },
    })
}
