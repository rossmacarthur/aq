# aq

Extend `jq` for any data format. Currently supports JSON, TOML, and YAML.

## Getting started

Install `aq` using Cargo.

```
cargo install aq-cli
```

## Example usage

Filters are passed directly to  `jq` but you must specify the input and output
data format. For example with an input of TOML and output of JSON.
```sh
$ echo '[foo.bar]\nfield = 1337' | aq -i toml -o json '.foo'
```
```json
{
  "bar": {
    "field": 1337
  }
}
```

If not provided, the output format defaults to the input format. Additionally,
you can use `j` for JSON, `t` for TOML, and `y` for YAML for maximum brevity.
```sh
$ echo '[foo.bar]\nfield = 1337' | aq -it '.foo'
```
```toml
[foo]
field = 5
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
