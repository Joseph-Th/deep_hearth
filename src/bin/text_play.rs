//! Line-oriented, production-backed wilderness play adapter.

#[path = "text_play/mod.rs"]
mod text_play;

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use text_play::Session;

struct Arguments {
    seed: Option<u64>,
    load: Option<String>,
    script: Option<PathBuf>,
}

fn usage() -> &'static str {
    "Deep Hearth text play\n\
     cargo run --locked --bin text-play -- [--seed NUMBER | --load SLOT] [--script FILE]\n\
     Commands: help, look [direction], status, inventory, gather, gather N [decimal grams],\n\
     eat STACK, drink, drink SOURCE,\n\
     recipes [all], craft PROCESS_ID [batches] [from STACK] [with TOOL],\n\
     surveys [width], prospect MATERIAL METHOD [width] [with TOOL], knowledge [width],\n\
     mine MATERIAL GRAMS with TOOL [using METHOD], ore, dress PROCESS STACK GRAMS,\n\
     gear [all], assemble ID, take N, equip N, recover N,\n\
     storage [all], place-store ID, stash CONTAINER STACK, withdraw CONTAINER STACK,\n\
     fire, build-fire, feed-fire FIRE STACK,\n\
     n|s|e|w [ticks], sprint|sneak DIRECTION [ticks], crouch [ticks],\n\
     jump [direction] [ticks], wait [ticks|work|boundary], use [direction], break [direction],\n\
     shove|strike|prepare|release [direction], cancel, slot N, select N,\n\
     empty, drop, respawn, save SLOT, quit.\n\
     Directions for pointing: n, s, e, w, up, down, n-down, s-down, e-down, w-down.\n\
     Text play uses real 20 Hz control steps, never simulated turns."
}

fn parse_arguments() -> Result<Arguments, String> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Arguments {
        seed: None,
        load: None,
        script: None,
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--seed" => {
                let value = args.next().ok_or("--seed needs a number")?;
                parsed.seed = Some(value.parse().map_err(|_| "invalid --seed number")?);
            }
            "--load" => {
                parsed.load = Some(args.next().ok_or("--load needs a save slot")?);
            }
            "--script" => {
                parsed.script = Some(PathBuf::from(
                    args.next().ok_or("--script needs a file path")?,
                ));
            }
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            _ => return Err(format!("unrecognized argument: {flag}")),
        }
    }
    if parsed.seed.is_some() && parsed.load.is_some() {
        return Err("--seed and --load cannot be used together".to_owned());
    }
    Ok(parsed)
}

fn play_input(session: &mut Session, line: &str, echo_command: bool) -> Result<bool, String> {
    if line.trim().is_empty() || line.trim_start().starts_with('#') {
        return Ok(true);
    }
    if echo_command {
        println!("> {line}");
    }
    let (output, keep_playing) = session.command(line)?;
    print!("{output}");
    Ok(keep_playing)
}

fn play_script(session: &mut Session, path: &PathBuf) -> Result<(), String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("cannot open script {}: {error}", path.display()))?;
    for (index, line) in io::BufReader::new(file).lines().enumerate() {
        let line_number = index + 1;
        let line = line.map_err(|error| {
            format!(
                "cannot read script {}:{line_number}: {error}",
                path.display()
            )
        })?;
        if !play_input(session, &line, true)
            .map_err(|error| format!("script {}:{line_number}: {error}", path.display()))?
        {
            break;
        }
    }
    Ok(())
}

fn run(args: Arguments) -> Result<(), String> {
    let mut session = if let Some(slot) = args.load {
        Session::load(&slot)?
    } else {
        let seed = args.seed.unwrap_or(24_318);
        Session::new(seed)?
    };
    print!("{}", session.opening());
    if let Some(script) = args.script {
        return play_script(&mut session, &script);
    }
    println!("Enter a command, or help for available actions.");
    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout()
            .flush()
            .map_err(|error| format!("terminal output failed: {error}"))?;
        let mut line = String::new();
        if stdin
            .read_line(&mut line)
            .map_err(|error| format!("terminal input failed: {error}"))?
            == 0
        {
            break;
        }
        match play_input(&mut session, line.trim_end(), false) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => println!("Rejected: {error}"),
        }
    }
    Ok(())
}

fn main() {
    let args = parse_arguments().unwrap_or_else(|error| {
        eprintln!("text-play: {error}\n{}", usage());
        std::process::exit(2);
    });
    if let Err(error) = run(args) {
        eprintln!("text-play: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
#[path = "text_play/script_tests.rs"]
mod tests;
