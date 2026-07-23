mod helpers;

use crate::helpers::{aq, assert_parity, assert_parity_err, run, Stdin};

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
        let mut aq = aq();
        let output = run(aq.args(args), input).unwrap();
        eprintln!("aq: {output:?}");
        assert!(output.status.success());
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

    // from json to toml
    assert(
        &["--output", "toml", "."],
        "{\"foo\":1337}\n",
        "foo = 1337\n",
    );
}

#[test]
fn yaml_sanity() {
    #[track_caller]
    fn assert(args: &[&str], input: impl Into<Stdin>, expected: &str) {
        let mut aq = aq();
        let output = run(aq.args(args), input).unwrap();
        eprintln!("aq: {output:?}");
        assert!(output.status.success());
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

    // from json to yaml
    assert(
        &["--output", "yaml", "."],
        "{\"foo\":1337}\n",
        "foo: 1337\n",
    );
}
