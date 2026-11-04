# waymaker-lib [![License](https://img.shields.io/github/license/felipecm-br/waymaker)](LICENSE)

Waymaker is a fuzzy searcher, powered by nucleo and written in rust.

![screen1](assets/screen1.png)

## Features

- Matching with [nucleo](https://github.com/helix-editor/nucleo).
- Declarative configuration which can be sourced from a [toml file](../waymaker-cli/assets/config.toml), or overridden using an intuitive [syntax](../waymaker-cli/assets/docs/options.md) for specifying command line options.
- Interactive preview supports color, scrolling, wrapping, multiple layouts, and even entering into an interactive view.
- [FZF](https://github.com/junegunn/fzf)-inspired actions.
- Column support: Split input lines into multiple columns, that you can dynamically search, filter, highlight, return etc.
- Available as a rust library to use in your own code.

## Library

Waymaker can also be used as a library.

```sh
cargo add waymaker
```

### Example

Here is how to use `Matchmaker` to select from a list of strings.

```rust
use waymaker::nucleo::{Indexed, Worker};
use waymaker::{MatchError, Matchmaker, Result, Selector};

#[tokio::main]
async fn main() -> Result<()> {
    let items = vec!["item1", "item2", "item3"];

    let worker = Worker::new_single_column();
    worker.append(items);
    let selector = Selector::new(Indexed::identifier);
    let wm = Matchmaker::new(worker, selector);

    match wm.pick_default().await {
        Ok(v) => {
            println!("{}", v[0]);
        }
        Err(err) => match err {
            MatchError::Abort(1) => {
                eprintln!("cancelled");
            }
            _ => {
                eprintln!("Error: {err}");
            }
        },
    }

    Ok(())
}
```

For more information, check out the [examples](./examples/) and [Architecture.md](./ARCHITECTURE.md)
