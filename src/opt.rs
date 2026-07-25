use std::env;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;

use crate::ExitCode;
use crate::Transcoder;

#[derive(Debug, Default)]
pub struct Opt {
    /// The program name
    pub prog: Option<OsString>,
    /// Info about some jq options that are set
    pub info: Info,
    /// Arguments to pass to jq
    pub args: Vec<OsString>,
    /// Files to read input from
    pub files: Vec<OsString>,
    /// Whether to force color output (pass -C to jq)
    pub force_color_output: bool,
}

#[derive(Debug, Default)]
pub struct Info {
    pub filter: bool,
    pub null_input: bool,
    pub raw_input: bool,
    pub raw_output: bool,
    pub color_output: bool,
    pub monochrome_output: bool,
    pub args: bool,
    pub jsonargs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Json,
    Toml,
    Yaml,
}

pub fn usage(prog: Option<&OsStr>, code: ExitCode) -> ! {
    let fmt = Format::from_prog(prog);
    let prog = prog.and_then(|p| p.to_str()).unwrap_or("aq");

    const USAGE: &str = {
        r#"aq - command line JSON / TOML / YAML processor
     built on top of jq by transcoding to and from JSON

Usage: $PROG [options] <jq filter> [file...]

Options:
  -i, --input FMT   the input data format [default: auto]
  -o, --output FMT  the output data format [default: json]
  ...               other options are passed directly to jq

Where FMT is one of json, toml, or yaml. Formats can also be specified
using the shorthand j, t, or y. When the input format is not specified,
it is inferred from the file extension of the input files, if stdin is
used then the input format defaults to json. aq is a multi-call binary,
when symlinked to tq or yq the input format will default to toml or yaml
respectively.

All other options are passed directly to jq. See jq --help or the jq man
page for more details. Some options may only be compatible with certain
input or output formats. aq does make some effort to detect incompatible
options but some may simply have no effect."#
    };

    const TAIL: &str = "See https://github.com/rossmacarthur/aq for more information";

    const AQ_EXAMPLES: &str = r#"
Example (input TOML, output JSON):

    $ echo -e '[foo]\nbar = 1337' | aq -it .foo
    {
      "bar": 1337
    }

Example (input JSON, output TOML):

    $ aq -n --arg name "Alice" '{name: $name}' --output toml
    name = "Alice"
"#;

    const JQ_EXAMPLES: &str = r#"
Example (input JSON, output JSON):

    $ echo '{"foo": 0}' | $PROG .
    {
      "foo": 0
    }
"#;

    const TQ_EXAMPLES: &str = r#"
Example (input TOML, output JSON):

    $ echo -e '[foo]\nbar = 1337' | $PROG .foo
    {
      "bar": 1337
    }

Example (input TOML, output TOML):

    $ echo -e '[foo]\nbar = 1337' | $PROG -ot .foo
    bar = 1337
"#;

    const YQ_EXAMPLES: &str = r#"
Example (input YAML, output JSON):

    $ echo -e 'foo: 1337' | $PROG .
    {
        "foo": 1337
    }

Example (input YAML, output YAML):

    $ echo -e 'foo: 1337' | $PROG -oy .
    foo: 1337
"#;

    let examples = match fmt {
        Some(Format::Json) => JQ_EXAMPLES,
        Some(Format::Toml) => TQ_EXAMPLES,
        Some(Format::Yaml) => YQ_EXAMPLES,
        None => AQ_EXAMPLES,
    };

    eprintln!(
        "{usage}\n{examples}\n{TAIL}",
        usage = USAGE.replace("$PROG", prog),
        examples = examples.replace("$PROG", prog),
    );
    process::exit(code.into());
}

pub fn parse() -> Result<Transcoder> {
    let mut iter = env::args_os();

    let prog = iter
        .next()
        .and_then(|p| PathBuf::from(p).file_name().map(OsStr::to_owned));

    let mut input: Option<Format> = None;
    let mut output: Option<Format> = None;

    let mut info = Info::default();
    let mut args = Vec::new();
    let mut files = Vec::new();

    while let Some(arg) = iter.next() {
        match arg.as_os_str().to_str() {
            Some("-h" | "--help") => {
                usage(prog.as_deref(), ExitCode::Success);
            }
            Some("-V" | "--version") => {
                println!("aq {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            Some("--") => {
                // This signals that all remaining arguments are not options
                files.extend(iter);
                break;
            }
            Some("-i" | "--input") => {
                let fmt = iter.next().with_context(err_input_missing_arg)?;
                input = Some(Format::from_os_str(&fmt).with_context(err_input_bad_format)?);
            }
            Some(arg) if arg.starts_with("-i") => {
                input = Some(Format::from_str(&arg[2..]).with_context(err_input_bad_format)?);
            }
            Some("-o" | "--output") => {
                let fmt = iter.next().with_context(err_output_missing_arg)?;
                output = Some(Format::from_os_str(&fmt).with_context(err_output_bad_format)?);
            }
            Some(arg) if arg.starts_with("-o") => {
                output = Some(Format::from_str(&arg[2..]).with_context(err_output_bad_format)?);
            }

            // Remaining options are passed directly to jq, but we do need to
            // track some of them in order to validate they are compatible with
            // the input and output formats, and to control some of the ways
            // that we invoke jq.
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
            Some("--indent") => {
                let a = iter.next().with_context(err_indent_missing_arg)?;
                args.push(arg);
                args.push(a);
            }
            Some("--library-path" | "-L") => {
                let a = iter.next().with_context(err_library_path_missing_arg)?;
                args.push(arg);
                args.push(a);
            }
            Some(opt @ ("--arg" | "--argjson" | "--slurpfile" | "--rawfile")) => {
                let msg = || format!("{opt} requires two arguments, e.g. {opt} name value");
                let a = iter.next().with_context(msg)?;
                let b = iter.next().with_context(msg)?;
                args.push(arg);
                args.push(a);
                args.push(b);
            }
            Some(opt) if opt.starts_with("--") => {
                args.push(arg);
            }
            Some(opt) if opt.starts_with("-L") => {
                args.push(arg);
            }
            Some(opt) if opt != "-" && opt.starts_with('-') => {
                if opt.contains('i') {
                    bail!("-i must not be clustered with other short options");
                }
                if opt.contains('o') {
                    bail!("-o must not be clustered with other short options");
                }
                if opt.contains('n') {
                    info.null_input = true;
                }
                if opt.contains('R') {
                    info.raw_input = true;
                }
                if opt.contains('r') {
                    info.raw_output = true;
                }
                if opt.contains("C") {
                    info.color_output = true;
                }
                if opt.contains("M") {
                    info.monochrome_output = true;
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

    // If --args or --jsonargs is set, then all remaining arguments are not
    // input files, but rather arguments to pass to jq
    if info.args || info.jsonargs {
        args.append(&mut files);
    }

    let input = input
        .or_else(|| Format::from_prog(prog.as_deref()))
        .or_else(|| Format::from_files(&files))
        .unwrap_or(Format::Json);

    let output = output.unwrap_or(Format::Json);

    for (arg, is_set) in [
        ("-R / --raw-input", info.raw_input),
        ("--args", info.args),
        ("--jsonargs", info.jsonargs),
    ] {
        if is_set && input != Format::Json {
            bail!("{arg} is only compatible with JSON input")
        }
    }
    if info.raw_output && output != Format::Json {
        bail!("-r is only compatible with JSON output")
    }

    let force_color_output = output == Format::Json
        && !info.monochrome_output
        && !info.color_output
        && !env::var_os("NO_COLOR")
            .and_then(|v| v.to_str().map(|s| !s.is_empty()))
            .unwrap_or(false);

    Ok(Transcoder {
        input,
        output,
        opt: Opt {
            prog,
            info,
            args,
            files,
            force_color_output,
        },
    })
}

impl Format {
    fn from_prog(prog: Option<&OsStr>) -> Option<Self> {
        prog.and_then(|b| match b.to_str() {
            Some("aq") => None,
            Some("jq") => Some(Format::Json),
            Some("tq") => Some(Format::Toml),
            Some("yq") => Some(Format::Yaml),
            _ => None,
        })
    }

    fn from_files(files: &[OsString]) -> Option<Self> {
        for path in files {
            match Path::new(path).extension().and_then(|ext| ext.to_str()) {
                Some("json" | "jsonl" | "ndjson") => return Some(Format::Json),
                Some("toml") => return Some(Format::Toml),
                Some("yaml") | Some("yml") => return Some(Format::Yaml),
                _ => {}
            }
        }
        None
    }

    fn from_os_str(s: &OsStr) -> Option<Self> {
        Self::from_str(s.to_str()?)
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "json" | "j" => Some(Format::Json),
            "toml" | "t" => Some(Format::Toml),
            "yaml" | "y" => Some(Format::Yaml),
            _ => None,
        }
    }
}

fn err_input_missing_arg() -> String {
    "-i / --input requires an argument: e.g. -it or --input toml".into()
}

fn err_input_bad_format() -> String {
    "-i / --input takes one of 'json', 'toml', 'yaml', or shorthand 'j', 't', 'y'".into()
}

fn err_output_missing_arg() -> String {
    "-o / --output requires an argument: e.g. -ot or --output toml".into()
}

fn err_output_bad_format() -> String {
    "-o / --output takes one of 'json', 'toml', 'yaml', or shorthand 'j', 't', 'y'".into()
}

fn err_indent_missing_arg() -> String {
    "--indent requires an argument: e.g. --indent 4".into()
}

fn err_library_path_missing_arg() -> String {
    " -L / --library-path requires an argument: e.g. -L /search/path".into()
}
