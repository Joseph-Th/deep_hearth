//! Command-line options for the block-world viewer.

use std::path::PathBuf;

use deep_hearth::core::time::Season;

pub const USAGE: &str = "\
hearth-view: render a generated Deep Hearth world

Usage: hearth-view [options]

  --seed <u64>          world seed (default 1)
  --radius <chunks>     generated chunk radius around the spawn, 1-16 (default 6)
  --season <name>       spring | summer | autumn | winter (default summer)
  --hour <0-24>         hours since dawn on the simulation day cycle (default 4)
  --yaw <degrees>       initial camera heading, 0 looks toward +Z (default 35)
  --pitch <degrees>     initial camera pitch (default -18)
  --rise <blocks>       initial camera height above the spawn eye (default 14)
  --screenshot <file>   render one frame to a PNG file and exit
  --size <W>x<H>        screenshot size (default 1600x900)

Window controls: click to capture the mouse, Esc to release, WASD to move, Space/C to rise
and sink, Shift to move faster, [ and ] to change the hour, T to let time run.";

#[derive(Clone, Debug)]
pub struct Options {
    pub seed: u64,
    pub radius: i64,
    pub season: Season,
    pub hour: f32,
    pub yaw_degrees: f32,
    pub pitch_degrees: f32,
    pub rise: f32,
    pub screenshot: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            seed: 1,
            radius: 6,
            season: Season::Summer,
            hour: 4.0,
            yaw_degrees: 35.0,
            pitch_degrees: -18.0,
            rise: 14.0,
            screenshot: None,
            width: 1600,
            height: 900,
        }
    }
}

pub enum Parsed {
    Run(Options),
    Help,
}

pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Parsed, String> {
    let mut options = Options::default();
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            return Ok(Parsed::Help);
        }
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        apply(&mut options, &flag, &value)?;
    }
    Ok(Parsed::Run(options))
}

fn apply(options: &mut Options, flag: &str, value: &str) -> Result<(), String> {
    match flag {
        "--seed" => options.seed = number(flag, value)?,
        "--radius" => {
            options.radius = number(flag, value)?;
            if !(1..=16).contains(&options.radius) {
                return Err("--radius must be between 1 and 16 chunks".to_owned());
            }
        }
        "--season" => options.season = season(value)?,
        "--hour" => {
            options.hour = finite_number(flag, value)?;
            if !(0.0..=24.0).contains(&options.hour) {
                return Err("--hour must be between 0 and 24".to_owned());
            }
        }
        "--yaw" => options.yaw_degrees = finite_number(flag, value)?.rem_euclid(360.0),
        "--pitch" => options.pitch_degrees = finite_number(flag, value)?.clamp(-89.0, 89.0),
        "--rise" => options.rise = finite_number(flag, value)?,
        "--screenshot" => options.screenshot = Some(PathBuf::from(value)),
        "--size" => (options.width, options.height) = size(value)?,
        _ => return Err(format!("unknown option {flag}; see --help")),
    }
    Ok(())
}

fn number<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("{flag} expects a number, got {value:?}"))
}

fn finite_number(flag: &str, value: &str) -> Result<f32, String> {
    let parsed = number::<f32>(flag, value)?;
    if !parsed.is_finite() {
        return Err(format!("{flag} requires a finite number"));
    }
    Ok(parsed)
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;

fn season(value: &str) -> Result<Season, String> {
    match value.to_ascii_lowercase().as_str() {
        "spring" => Ok(Season::Spring),
        "summer" => Ok(Season::Summer),
        "autumn" | "fall" => Ok(Season::Autumn),
        "winter" => Ok(Season::Winter),
        _ => Err(format!("unknown season {value:?}")),
    }
}

fn size(value: &str) -> Result<(u32, u32), String> {
    let (width, height) = value
        .split_once('x')
        .ok_or_else(|| format!("--size expects WxH, got {value:?}"))?;
    let width: u32 = number("--size", width)?;
    let height: u32 = number("--size", height)?;
    if !(64..=8192).contains(&width) || !(64..=8192).contains(&height) {
        return Err("--size dimensions must be between 64 and 8192".to_owned());
    }
    Ok((width, height))
}
