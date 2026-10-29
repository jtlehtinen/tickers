# tick-rs

Realtime ticker data in your terminal.

> [!NOTE]
> This is a fork of [tarkah/tickrs](https://github.com/tarkah/tickrs) that has diverged
> considerably from the original. It is not published to crates.io or any package manager.

![tickers showing a 1D candle chart with volumes and a sidebar of stock tabs](./assets/shot.png)

## Installation

Clone the repository and build it:

```
git clone https://github.com/jtlehtinen/tickers
cd tickers
cargo build --release
```

The binary is at `target/release/tickers`.

## Config File

On first run, tickers writes a commented default config to `tickers/config.yml` in your
platform's config directory:

- Linux: `~/.config/tickers/config.yml`
- macOS: `~/Library/Application Support/tickers/config.yml`
- Windows: `%APPDATA%\tickers\config.yml`

It accepts the same options as the command line; run `tickers --help` to list them.

## Price Alerts

Press `a` to open the alerts pane, `n` to add an alert (`SYMBOL PRICE`, e.g. `AAPL 200`)
and `d` to delete the selected one. An alert triggers once, when the price reaches or
crosses its target, and shows a desktop notification with `notify-send`. The symbol
doesn't need a tab, its price is still checked every 15 seconds.

Alerts are saved to `tickers/alerts.yml` in the data directory (`~/.local/share` on Linux).

## Acknowledgments
- [ratatui](https://github.com/ratatui/ratatui) - TUI library for Rust, and [fdehau](https://github.com/fdehau) / [tui-rs](https://github.com/fdehau/tui-rs) before it
- [cjbassi](https://github.com/cjbassi) / [ytop](https://github.com/cjbassi/ytop) - thanks for the inspiration!
