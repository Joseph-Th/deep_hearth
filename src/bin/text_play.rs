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
     Commands: help, look [direction], status, inventory, gather, gather N [grams],\n\
     n|s|e|w [ticks], wait [ticks], use [direction], break [direction],\n\
     shove|strike|prepare|release [direction], cancel, slot N, select N,\n\
     empty, drop, respawn, save SLOT, quit.\n\
     Directions for pointing: n, s, e, w, up, down.\n\
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

fn play_input(session: &mut Session, line: &str, echo_command: bool) -> bool {
    if line.trim().is_empty() || line.trim_start().starts_with('#') {
        return true;
    }
    if echo_command {
        println!("> {line}");
    }
    match session.command(line) {
        Ok((output, keep_playing)) => {
            print!("{output}");
            keep_playing
        }
        Err(error) => {
            println!("Rejected: {error}");
            true
        }
    }
}

fn play_script(session: &mut Session, path: &PathBuf) -> Result<(), String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("cannot open script {}: {error}", path.display()))?;
    for line in io::BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("cannot read script: {error}"))?;
        if !play_input(session, &line, true) {
            break;
        }
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args = parse_arguments()?;
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
        if !play_input(&mut session, line.trim_end(), false) {
            break;
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("text-play: {error}\n{}", usage());
        std::process::exit(2);
    }
}
