use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context as _, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone)]
pub enum Stdin {
    Feed(String),
    Close,
}

impl From<&str> for Stdin {
    fn from(s: &str) -> Self {
        Stdin::Feed(s.to_owned())
    }
}

impl<T> From<Option<T>> for Stdin
where
    T: Into<String>,
{
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(s) => Stdin::Feed(s.into()),
            None => Stdin::Close,
        }
    }
}

#[track_caller]
pub fn assert_parity<I>(args: I, stdin: impl Into<Stdin>)
where
    I: IntoIterator<Item = &'static str> + Clone,
{
    check_parity(args, stdin, |jq_out, aq_out| {
        // Check that the test case actually produced some output
        assert!(jq_out.status.success(), "jq failed to run: {jq_out:?}");
        assert!(aq_out.status.success(), "aq failed to run: {aq_out:?}");
        assert!(jq_out.stdout.trim() != "null");
        assert!(aq_out.stdout.trim() != "null");
    });
}

#[track_caller]
pub fn assert_parity_err<I>(exit_code: i32, args: I, stdin: impl Into<Stdin>)
where
    I: IntoIterator<Item = &'static str> + Clone,
{
    check_parity(args, stdin, |_, aq_out| {
        assert_eq!(
            aq_out.status.code().unwrap_or(-1),
            exit_code,
            "exit code mismatch"
        );
    });
}

#[track_caller]
pub fn check_parity<I, F>(args: I, stdin: impl Into<Stdin>, check_more: F)
where
    I: IntoIterator<Item = &'static str> + Clone,
    F: Fn(&StringOutput, &StringOutput),
{
    let stdin = stdin.into();
    let mut jq = jq();
    jq.args(args.clone());
    let mut aq = aq();
    aq.args(args);
    let jq_out = run(&mut jq, stdin.clone()).expect("jq failed to run");
    let aq_out = run(&mut aq, stdin).expect("aq failed to run");
    assert_eq!(
        aq_out, jq_out,
        "output mismatch: \njq: {jq_out:?}\naq: {aq_out:?}"
    );
    check_more(&jq_out, &aq_out);
}

pub fn aq() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aq"))
}

pub fn jq() -> Command {
    Command::new("jq")
}

/// Runs `cmd`, feeding it `stdin` (or closing stdin immediately if `None`,
/// same as `< /dev/null`), and returns its combined stdout+stderr and exit
/// status.
pub fn run(cmd: &mut Command, stdin: impl Into<Stdin>) -> Result<StringOutput> {
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.current_dir(PathBuf::from_iter([
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "fixtures",
    ]));
    let mut child = cmd.spawn().context("failed to spawn")?;
    match stdin.into() {
        Stdin::Feed(data) => {
            let mut input = child.stdin.take().unwrap();
            input
                .write_all(data.as_bytes())
                .context("failed to write stdin")?;
        }
        Stdin::Close => drop(child.stdin.take()),
    }
    let output = child.wait_with_output().expect("failed to wait");
    let status = output.status;
    let stdout = String::from_utf8_lossy(&output.stdout).into();
    let stderr = String::from_utf8_lossy(&output.stderr).into();
    Ok(StringOutput {
        status,
        stdout,
        stderr,
    })
}
