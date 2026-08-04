mod helpers;

use std::io;
use std::io::prelude::*;
use std::process::Command;
use std::process::Stdio;

use crate::helpers::aq;
use crate::helpers::assert_parity;
use crate::helpers::assert_parity_err;
use crate::helpers::jq;
use crate::helpers::run;
use crate::helpers::Stdin;
use crate::helpers::StringOutput;

/// aq should behave identically to jq for the same arguments and input if
/// we haven't specified any input / output format conversions
#[test]
fn json_jq_parity() {
    let json = r#"{"foo":1337}"#;
    let jsonl = r#"{"foo":1337}
{"foo":42}
"#;

    assert_parity([], Stdin::Close);
    assert_parity(["."], json);
    assert_parity([".foo"], json);

    assert_parity([".", "bar.json", "baz.json"], Stdin::Close);
    assert_parity([".", "bar.json", "-", "baz.json"], json);

    assert_parity(["-n", r#"{"foo":.}"#], Stdin::Close);
    assert_parity(["--null-input", r#"{"foo":.}"#], Stdin::Close);
    assert_parity(["-n", r#"{"foo":.}"#], json);
    assert_parity(["--null-input", r#"{"foo":.}"#], json);

    assert_parity(["-n", "input"], json);
    assert_parity(["-n", "input", "bar.json"], Stdin::Close);

    assert_parity(["-R", "."], "1337");
    assert_parity(["--raw-input", "."], "1337");

    assert_parity(["-c", "."], json);
    assert_parity(["--compact-output", "."], json);
    assert_parity(["-r", ".foo"], json);
    assert_parity(["--raw-output", ".foo"], json);
    assert_parity(["--raw-output0", ".foo"], jsonl);
    assert_parity(["-j", ".foo"], jsonl);
    assert_parity(["--join-output", ".foo"], jsonl);

    assert_parity(["-a", ".foo"], r#"{"foo":"💚"}"#);
    assert_parity(["--ascii-output", ".foo"], r#"{"foo":"💚"}"#);

    assert_parity(["-S", "."], r#"{"foo":1,"bar":2}"#);
    assert_parity(["--sort-keys", "."], r#"{"foo":1,"bar":2}"#);
    assert_parity(["-C", "."], json);
    assert_parity(["--color-output", "."], json);
    assert_parity(["-M", "."], json);
    assert_parity(["--monochrome-output", "."], json);
    assert_parity(["--tab", "."], json);
    assert_parity(["--indent", "6", "."], json);

    // TODO: --unbuffered
    // TODO --stream
    // TODO --stream-errors

    assert_parity(["--seq", "."], "\x1e{\"foo\":1337}\n\x1e{\"foo\":42}\n");

    assert_parity(["-f", "filter.jq"], json);
    assert_parity(["--from-file", "filter.jq"], json);

    assert_parity(["-L./fixtures", "."], json);
    assert_parity(["-L", "./fixtures", "."], json);
    assert_parity(["--library-path", "filter.jq"], json);

    assert_parity(
        ["-n", r#"{"foo":$foo}"#, "--arg", "foo", "bar"],
        Stdin::Close,
    );
    assert_parity(
        ["-n", r#"{"foo":$foo}"#, "--argjson", "foo", "{}"],
        Stdin::Close,
    );
    assert_parity(
        [
            "-n",
            r#"{"foo":$foo}"#,
            "--slurpfile",
            "foo",
            "slurpfile.json",
        ],
        Stdin::Close,
    );
    assert_parity(
        [
            "-n",
            r#"{"foo":$foo}"#,
            "--slurpfile",
            "foo",
            "slurpfile.json",
        ],
        Stdin::Close,
    );
    assert_parity(
        ["-n", r#"{"foo":$foo}"#, "--rawfile", "foo", "rawfile.txt"],
        Stdin::Close,
    );
    assert_parity(
        [
            "-nc",
            r#"{"foo":$ARGS.positional}"#,
            "--args",
            "1337",
            "bar",
        ],
        Stdin::Close,
    );
    assert_parity(
        [
            "-nc",
            r#"{"foo":$ARGS.positional}"#,
            "--jsonargs",
            "1337",
            "\"bar\"",
            "[\"baz\",42]",
        ],
        Stdin::Close,
    );
    assert_parity(
        [
            "-nc",
            "--args",
            r#"{"foo":$ARGS.positional}"#,
            "1337",
            "bar",
        ],
        Stdin::Close,
    );
    assert_parity(
        ["-nc", r#"{"foo":$ARGS.positional}"#, "--args", "-r", "bar"],
        Stdin::Close,
    );

    assert_parity([".", "bar.json", "--sort-keys"], Stdin::Close);
    assert_parity(["--compact-output", ".", "bar.json"], Stdin::Close);
    assert_parity([".", "bar.json", "baz.json", "-c"], Stdin::Close);

    assert_parity(["-e", ".foo"], json);
    assert_parity(["--exit-status", ".foo"], json);
    assert_parity_err(1, ["-e", ".bar"], json);
    assert_parity_err(1, ["--exit-status", ".bar"], json);

    assert_parity([".", "--", "--sort-keys"], Stdin::Close);
}

#[test]
fn toml_sanity() {
    #[track_caller]
    fn assert(args: &[&str], input: impl Into<Stdin>, expected: &str) {
        let output = run(aq().args(args), input).unwrap();
        eprintln!("aq: {output:?}");
        assert_eq!(output.code, Some(0));
        assert_eq!(output.signal, None);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, "");
    }

    // from toml to json
    assert(&["--input", "toml", "-c", ".foo"], "foo = 1337\n", "1337\n");
    assert(&["-it", "-c", ".foo"], "foo = 1337\n", "1337\n");
    assert(&["-c", ".", "bar.toml"], Stdin::Close, "{\"bar\":42}\n");
    assert(&[".bar", "-", "bar.toml"], "bar = 1337\n", "1337\n42\n");

    // from toml to toml
    assert(&["-it", "-ot", "."], "foo = 1337\n", "foo = 1337\n");
    assert(&["-it", "-ot", ".foo"], "foo = 1337\n", "1337\n");
    assert(&["-ot", ".", "bar.toml"], Stdin::Close, "bar = 42\n");
    assert(&["-ot", ".bar", "bar.toml"], Stdin::Close, "42\n");
    assert(&["-ot", ".unknown", "bar.toml"], Stdin::Close, "\n");
    assert(&["-ot", "select(false)|.", "bar.toml"], Stdin::Close, "\n");
    assert(
        &["-ot", ".", "bar.toml", "baz.toml"],
        "foo = 1337",
        "bar = 42\nbaz = 7\n",
    );
    assert(
        &["-it", "-ot", "-s", "."],
        "foo = 1337\nbar = 42\nbaz = 7\n",
        "[{ bar = 42, baz = 7, foo = 1337 }]\n",
    );

    // from json to toml
    assert(
        &["--output", "toml", "."],
        "{\"foo\":1337}\n",
        "foo = 1337\n",
    );
    assert(
        &["-ot", "."],
        "{\"foo\":1337}\n{\"bar\":42}\n",
        "foo = 1337\nbar = 42\n",
    );
    assert(
        &["-ot", "-s", "."],
        "{\"foo\":1337}\n{\"bar\":42}\n",
        "[{ foo = 1337 }, { bar = 42 }]\n",
    );
}

#[test]
fn yaml_sanity() {
    #[track_caller]
    fn assert(args: &[&str], input: impl Into<Stdin>, expected: &str) {
        let output = run(aq().args(args), input).unwrap();
        eprintln!("aq: {output:?}");
        assert_eq!(output.code, Some(0));
        assert_eq!(output.signal, None);
        assert_eq!(output.stdout, expected);
        assert_eq!(output.stderr, "");
    }

    // from yaml to json
    assert(&["--input", "yaml", "-c", ".foo"], "foo: 1337\n", "1337\n");
    assert(&["-iy", "-c", ".foo"], "foo: 1337\n", "1337\n");
    assert(&["-c", ".", "bar.yaml"], Stdin::Close, "{\"bar\":42}\n");
    assert(&[".bar", "-", "bar.yaml"], "bar: 1337\n", "1337\n42\n");

    // from yaml to yaml
    assert(&["-iy", "-oy", "."], "foo: 1337\n", "foo: 1337\n");
    assert(&["-iy", "-oy", ".foo"], "foo: 1337\n", "1337\n");
    assert(&["-oy", ".", "bar.yaml"], Stdin::Close, "bar: 42\n");
    assert(&["-oy", ".bar", "bar.yaml"], Stdin::Close, "42\n");
    assert(&["-oy", ".unknown", "bar.yaml"], Stdin::Close, "null\n");
    assert(&["-oy", "select(false)|.", "bar.yaml"], Stdin::Close, "\n");
    assert(
        &["-oy", ".", "bar.yaml", "baz.yaml"],
        "foo: 1337",
        "---\nbar: 42\n---\nbaz: 7\n",
    );
    assert(
        &["-iy", "-oy", "."],
        "---\nfoo: 1337\n---\nbar: 42\n---\nbaz: 7\n",
        "---\nfoo: 1337\n---\nbar: 42\n---\nbaz: 7\n",
    );
    assert(
        &["-iy", "-oy", "-s", "."],
        "---\nfoo: 1337\n---\nbar: 42\n---\nbaz: 7\n",
        "- foo: 1337\n- bar: 42\n- baz: 7\n",
    );

    // from json to yaml
    assert(
        &["--output", "yaml", "."],
        "{\"foo\":1337}\n",
        "foo: 1337\n",
    );
    assert(
        &["-oy", "."],
        "{\"foo\":1337}\n{\"bar\":42}\n",
        "---\nfoo: 1337\n---\nbar: 42\n",
    );
    assert(
        &["-oy", "-s", "."],
        "{\"foo\":1337}\n{\"bar\":42}\n",
        "- foo: 1337\n- bar: 42\n",
    );
}

#[test]
fn parity_broken_output_pipe() {
    fn run(cmd: &mut Command) -> io::Result<StringOutput> {
        let mut child = cmd
            .arg("--unbuffered")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();

        input.write_all(b"{}\n")?;
        input.flush()?;

        let mut byte = [0u8; 1];
        output.read_exact(&mut byte)?;
        drop(output);

        // now write again while the output pipe is closed
        input.write_all(b"{}\n")?;
        input.flush()?;
        drop(input);

        let output = child.wait_with_output()?;
        Ok(StringOutput::from(output))
    }

    let jq_out = run(&mut jq()).expect("failed to run jq");
    let aq_out = run(&mut aq()).expect("failed to run aq");
    assert_eq!(
        aq_out, jq_out,
        "output mismatch: \njq: {jq_out:?}\naq: {aq_out:?}"
    );
}

#[test]
fn err_bad_filter() {
    let output = run(aq().args(["-n", "{"]), Stdin::Close).unwrap();
    goldie::assert!(output);
}

#[test]
fn err_bad_filter_and_missing_file() {
    let output = run(aq().args(["-n", "{", "missing1.json"]), Stdin::Close).unwrap();
    goldie::assert!(output);
}

#[test]
fn err_bad_filter_and_missing_file_3x() {
    let output = run(
        aq().args(["-n", "{", "missing1.json", "missing1.json", "missing1.json"]),
        Stdin::Close,
    )
    .unwrap();
    goldie::assert!(output);
}

#[test]
fn err_bad_filter_and_many_missing_files() {
    let mut aq = aq();
    aq.args(["-n", "{"]);
    for i in 1..=10 {
        aq.arg(format!("missing{i}.json"));
    }
    let output = run(&mut aq, Stdin::Close).unwrap();
    goldie::assert!(output);
}

#[test]
fn err_bad_input_toml() {
    let output = run(aq().args(["-it"]), "foo =").unwrap();
    goldie::assert!(output);
}

#[test]
fn err_bad_input_yaml() {
    let output = run(aq().args(["-iy"]), "foo:\n\tbar: 1").unwrap();
    goldie::assert!(output);
}
