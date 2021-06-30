# aq

[![Crates.io version](https://img.shields.io/crates/v/aq-cli.svg)](https://crates.io/crates/aq-cli)
[![Download](https://img.shields.io/github/v/release/rossmacarthur/aq?label=binary)](https://github.com/rossmacarthur/aq/releases/latest)
[![Build status](https://img.shields.io/github/workflow/status/rossmacarthur/aq/build/trunk)](https://github.com/rossmacarthur/aq/actions?query=workflow%3Abuild)

Extend [`jq`](https://stedolan.github.io/jq/manual) for any data format.
Currently supports JSON, TOML, and YAML.

## 📦 Installation

Pre-built binaries for 64-bit Linux, macOS, and Windows are provided. The
following script can be used to automatically detect your host system, download
the required artifact, and extract the `aq` binary to the given directory.

```sh
curl --proto '=https' -fLsS https://rossmacarthur.github.io/install/crate.sh \
    | bash -s -- --repo rossmacarthur/aq --to ~/.local/bin
```

Alternatively, you can download an artifact directly from the [the releases
page](https://github.com/rossmacarthur/aq/releases).

### Cargo

`aq` can be installed from [Crates.io](https://crates.io/crates/aq-cli)
using [Cargo](https://doc.rust-lang.org/cargo/), the Rust package manager.

```sh
cargo install aq-cli
```

## 🤸 Usage

By default `aq` behaves just like [`jq`](https://stedolan.github.io/jq/manual)
and operates on JSON.
```sh
$ echo '{"foo":{"bar": 1337}}' | aq .foo
```
```json
{
  "bar": 1337
}
```

But it also accepts options to specify the input and output format. For example
with a TOML input and a JSON output:

```sh
$ echo '[foo]\nbar = 1337' | aq -i toml -o json .foo
```
```json
{
  "bar": 1337
}
```

If not provided, the output format defaults to the input format. Additionally,
you can use `j` for JSON, `t` for TOML, and `y` for YAML for maximum brevity.
```sh
$ echo '[foo]\nbar = 1337' | aq -it .foo
```
```toml
bar = 1337
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
