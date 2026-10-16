use std::collections::HashMap;
use std::{fs, process};

use anyhow::{bail, format_err, Error};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::Parser;
use serde::Deserialize;

use crate::common::{ChartType, TimeFrame};
use crate::portfolio::Portfolio;
use crate::theme::Theme;
use crate::widget::KagiOptions;

pub fn resolve_opts() -> Opts {
    let mut opts = get_cli_opts();

    if let Ok(config_opts) = get_config_opts() {
        // Options
        opts.chart_type = opts.chart_type.or(config_opts.chart_type);
        opts.symbols = opts.symbols.or(config_opts.symbols);
        opts.time_frame = opts.time_frame.or(config_opts.time_frame);
        opts.update_interval = opts.update_interval.or(config_opts.update_interval);

        // Flags
        opts.enable_pre_post = opts.enable_pre_post || config_opts.enable_pre_post;
        opts.hide_help = opts.hide_help || config_opts.hide_help;
        opts.hide_prev_close = opts.hide_prev_close || config_opts.hide_prev_close;
        opts.hide_toggle = opts.hide_toggle || config_opts.hide_toggle;
        opts.show_volumes = opts.show_volumes || config_opts.show_volumes;
        opts.show_x_labels = opts.show_x_labels || config_opts.show_x_labels;
        opts.summary = opts.summary || config_opts.summary;
        opts.trunc_pre = opts.trunc_pre || config_opts.trunc_pre;

        // Theme
        opts.theme = config_opts.theme;

        // Kagi Options
        opts.kagi_options = config_opts.kagi_options;

        // Portfolio
        opts.portfolio = config_opts.portfolio;
    }

    opts
}

fn get_cli_opts() -> Opts {
    Opts::parse()
}

fn get_config_opts() -> Result<Opts, Error> {
    let config_dir = dirs_next::config_dir()
        .ok_or_else(|| format_err!("Could not get config directory"))?
        .join("tickrs");

    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    let config_path = config_dir.join("config.yml");

    if !config_path.exists() {
        let _ = fs::write(&config_path, DEFAULT_CONFIG);
    }

    let config = fs::read_to_string(&config_path)?;

    let opts = match serde_yaml::from_str::<Option<Opts>>(&config) {
        Ok(Some(opts)) => opts,
        Ok(None) => bail!("Empty config file"),
        Err(e) => {
            println!(
                "Error parsing config file, make sure format is valid\n\n  {}",
                e
            );
            process::exit(1);
        }
    };

    Ok(opts)
}

#[derive(Debug, Parser, Clone, Deserialize, Default)]
#[command(
    name = "tickrs",
    about = "Realtime ticker data in your terminal 📈",
    version
)]
#[serde(default)]
pub struct Opts {
    // Options
    //
    #[arg(
        short,
        long,
        value_parser = PossibleValuesParser::new(["line", "candle", "kagi"])
            .map(|s| s.parse::<ChartType>().unwrap()),
    )]
    /// Chart type to start app with [default: line]
    pub chart_type: Option<ChartType>,
    #[arg(short, long, value_delimiter = ',', num_args = 1..)]
    /// Comma separated list of ticker symbols to start app with
    pub symbols: Option<Vec<String>>,
    #[arg(
        short,
        long,
        value_parser = PossibleValuesParser::new(TimeFrame::tab_names())
            .map(|s| s.parse::<TimeFrame>().unwrap()),
    )]
    /// Use specified time frame when starting program and when new stocks are added [default: 1D]
    pub time_frame: Option<TimeFrame>,
    #[arg(short = 'i', long)]
    /// Interval to update data from API (seconds) [default: 1]
    pub update_interval: Option<u64>,

    // Flags
    //
    #[arg(short = 'p', long)]
    /// Enable pre / post market hours for graphs
    pub enable_pre_post: bool,
    #[arg(long)]
    /// Hide help icon in top right
    pub hide_help: bool,
    #[arg(long)]
    /// Hide previous close line on 1D chart
    pub hide_prev_close: bool,
    #[arg(long)]
    /// Hide toggle block
    pub hide_toggle: bool,
    #[arg(long)]
    /// Show volumes graph
    pub show_volumes: bool,
    #[arg(short = 'x', long)]
    /// Show x-axis labels
    pub show_x_labels: bool,
    #[arg(long)]
    /// Start in summary mode
    pub summary: bool,
    #[arg(long)]
    /// Truncate pre market graphing to only 30 minutes prior to markets opening
    pub trunc_pre: bool,

    #[arg(skip)]
    pub theme: Option<Theme>,
    #[arg(skip)]
    pub kagi_options: HashMap<String, KagiOptions>,
    #[arg(skip)]
    pub portfolio: Option<Portfolio>,
}

const DEFAULT_CONFIG: &str = "---
# List of ticker symbols to start app with
#symbols:
#  - SPY
#  - AMD

# Chart type to start app with
# Default is line
# Possible values: line, candle, kagi
#chart_type: candle

# Use specified time frame when starting program and when new stocks are added
# Default is 1D
# Possible values: 1D, 1W, 1M, 3M, 6M, 1Y, 5Y
#time_frame: 1D

# Interval to update data from API (seconds)
# Default is 1
#update_interval: 1

# Enable pre / post market hours for graphs
#enable_pre_post: true

# Hide help icon in top right
#hide_help: true

# Hide previous close line on 1D chart
#hide_prev_close: true

# Hide toggle block
#hide_toggle: true

# Show volumes graph
#show_volumes: true

# Show x-axis labels
#show_x_labels: true

# Start in summary mode
#summary: true

# Truncate pre market graphing to only 30 minutes prior to markets opening
#trunc_pre: true

# Ticker options for Kagi charts
#
# A map of each ticker with reversal and/or price fields (both optional). If no
# entry is defined for a symbol, a default of 'close' price and 1% for 1D and 4%
# for non-1D timeframes is used. This can be updated in the GUI by pressing 'e'
#
# reversal can be supplied as a single value, or a map on time frame to give each
# time frame a different reversal amount
#
# reversal.type can be 'amount' or 'pct'
#
# price can be 'close' or 'high_low'
#
#kagi_options:
#  SPY:
#    reversal:
#      type: amount
#      value: 5.00
#    price: close
#  AMD:
#    price: high_low
#  TSLA:
#    reversal:
#      type: pct
#      value: 0.08
#  NVDA:
#    reversal:
#      1D:
#        type: pct
#        value: 0.02
#      5Y:
#        type: pct
#        value: 0.10

# Apply a custom theme
#
# All colors are optional. If commented out / omitted, the color will get sourced
# from your terminal color scheme
#theme:
#  background: '#403E41'
#  gray: '#727072'
#  profit: '#ADD977'
#  loss: '#FA648A'
#  text_normal: '#FCFCFA'
#  text_primary: '#FFDA65'
#  text_secondary: '#79DBEA'
#  border_primary: '#FC9766'
#  border_secondary: '#FCFCFA'
#  border_axis: '#FC9766'
#  highlight_focused: '#FC9766'
#  highlight_unfocused: '#727072'

# Portfolio tracking
#
# Track your portfolio holdings to see profit/loss
# Each ticker is tracked with quantity and average purchase price
# Ticker symbols must match those in the symbols list in order to be visible
#
#portfolio:
#  AMD:
#    quantity: 1.3
#    average_price: 221
#  SPY:
#    quantity: 100
#    average_price: 450.25
";

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::Opts;
    use crate::common::{ChartType, TimeFrame};

    fn parse(args: &[&str]) -> Opts {
        Opts::try_parse_from(std::iter::once("tickrs").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn cli_is_valid() {
        Opts::command().debug_assert();
    }

    #[test]
    fn cli_parses_options() {
        let opts = parse(&["-c", "candle", "-t", "1W", "-i", "5", "-s", "MU,NVDA"]);

        assert_eq!(opts.chart_type, Some(ChartType::Candlestick));
        assert_eq!(opts.time_frame, Some(TimeFrame::Week1));
        assert_eq!(opts.update_interval, Some(5));
        assert_eq!(opts.symbols, Some(vec!["MU".into(), "NVDA".into()]));
    }

    #[test]
    fn cli_parses_symbols_like_structopt() {
        // Empty entries are kept here and dropped later by `normalize_symbols`
        let opts = parse(&["-s", "MU,NVDA,,AAOI"]);
        assert_eq!(
            opts.symbols,
            Some(vec!["MU".into(), "NVDA".into(), "".into(), "AAOI".into()])
        );

        // Multiple values per occurrence and repeated occurrences
        let opts = parse(&["-s", "MU", "NVDA", "--symbols", "AAOI"]);
        assert_eq!(
            opts.symbols,
            Some(vec!["MU".into(), "NVDA".into(), "AAOI".into()])
        );
    }

    #[test]
    fn cli_parses_flags() {
        let opts = parse(&["-p", "-x", "--summary", "--hide-help", "--trunc-pre"]);

        assert!(opts.enable_pre_post);
        assert!(opts.show_x_labels);
        assert!(opts.summary);
        assert!(opts.hide_help);
        assert!(opts.trunc_pre);
        assert!(!opts.show_volumes);
    }

    #[test]
    fn cli_rejects_invalid_values() {
        assert!(Opts::try_parse_from(["tickrs", "-c", "bar"]).is_err());
        assert!(Opts::try_parse_from(["tickrs", "-t", "2D"]).is_err());
    }
}
