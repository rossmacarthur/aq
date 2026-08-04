# aq

[![Crates.io Version](https://badgers.space/crates/version/aq-cli)](https://crates.io/crates/aq-cli)
[![Build Status](https://badgers.space/github/checks/rossmacarthur/aq/trunk?label=build)](https://github.com/rossmacarthur/aq/actions/workflows/build.yaml)

Extend [`jq`] for any data format. Currently supports JSON, TOML, and YAML.

## 🤸 Usage

By default **`aq`** behaves just like [`jq`] and operates on JSON.
```sh
$ echo '{"foo": 0}' | aq .
```
```json
{
  "foo": 0
}
```

But it also accepts options to specify the input and/or output format. For
example with a TOML input and a YAML output:

```sh
$ echo 'foo = 0' | aq -i toml -o yaml .
```
```yaml
foo: 0
```

Other options are forwarded to [`jq`] and can be used as normal.
```sh
$ aq -n --arg name "Alice" '{name: $name}' --output yaml
```
```yaml
name: Alice
```

When the input format is not specified, it is inferred from the file extension
of the input files, if stdin is used then the input format defaults to JSON. The
output format always defaults to JSON.

Additionally, you can use `j` for JSON, `t` for TOML, and `y` for YAML for
maximum brevity. The following uses `-it` to specify TOML input.
```sh
$ echo '[foo]\nbar = 1337' | aq -it .foo
```
```toml
bar = 1337
```

**`aq`** is also a multi-call binary, so if it is symlinked to `tq` or `yq` then
the input format will default to TOML or YAML respectively.

[`jq`]: https://jqlang.org/manual/

## 📦 Installation

### Homebrew

**`aq`** can be installed from my personal tap which includes pre-built
binaries.

```sh
brew install rossmacarthur/tap/aq
```

### Cargo

**`aq`** can be installed from
[Crates.io](https://crates.io/crates/aq-cli) using
[Cargo](https://doc.rust-lang.org/cargo/), the Rust package manager.

```sh
cargo install aq-cli
```

In some circumstances this can fail due to the fact that Cargo does not use
`Cargo.lock` file by default. You can force Cargo to use it using the `--locked`
option.

```sh
cargo install aq-cli --locked
```

### Pre-built binaries

Pre-built binaries for macOS (x86_64), Windows, Linux (x86_64) are provided.
These can be downloaded directly from the [the releases page].

Alternatively, the following script can be used to automatically detect your host
system, download the required artifact, and extract the **`aq`** binary to the
given directory.
```sh
curl --proto '=https' -fLsS https://rossmacarthur.github.io/install/crate.sh \
    | bash -s -- --repo rossmacarthur/aq --to ~/.local/bin
```

[the releases page]: https://github.com/rossmacarthur/aq/releases

## License

This project is distributed under the terms of both the MIT license and the
Apache License (Version 2.0).

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
