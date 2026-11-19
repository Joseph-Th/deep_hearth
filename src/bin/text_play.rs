//! Line-oriented, production-backed wilderness play adapter.

#[path = "text_play/mod.rs"]
mod text_play;

use std::hash::{BuildHasher, Hasher};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use deep_hearth::world::HabitatKind;
use text_play::Session;

struct Arguments {
    seed: Option<u64>,
    load: Option<String>,
    script: Option<PathBuf>,
    natural: bool,
    random: bool,
    habitat: Option<HabitatKind>,
}

fn usage() -> &'static str {
    "Deep Hearth text play\n\
     cargo run --locked --bin text-play -- [--seed NUMBER] [--natural] [--habitat dry|mixed|damp] [--script FILE]\n\
     cargo run --locked --bin text-play -- --random [--habitat dry|mixed|damp] [--script FILE]\n\
     cargo run --locked --bin text-play -- --load SLOT [--script FILE]\n\
     --natural uses seeded ecology, without guaranteed starter resources.\n\
     --random starts a natural world with a fresh, printed seed.\n\
     --habitat chooses a real region to test a specific resource/climate challenge.\n\
     Commands: help, look [direction], status, inventory, gather, gather N [decimal grams],\n\
     gather N [decimal grams] repeat COUNT (up to 32 batches; stops at interruptions),\n\
     eat STACK, drink, drink SOURCE,\n\
     recipes [all], craft PROCESS_ID [batches] [from STACK] [with TOOL],\n\
     surveys [width], prospect MATERIAL METHOD [width] [with TOOL], knowledge [width],\n\
     mine MATERIAL GRAMS with TOOL [using METHOD] [repeat COUNT], ore, dress PROCESS STACK GRAMS,\n\
     gear [all], assemble ID, take N, equip N, recover N,\n\
     storage [all], place-store ID, stash CONTAINER STACK [grams], withdraw CONTAINER STACK [grams],\n\
     plan-at STACK X Y Z, build-at STACK X Y Z (supported framing or panel),\n\
     fire, build-fire, feed-fire FIRE STACK,\n\
     n|s|e|w [ticks], trek DIRECTION [cells] [steady], climb DIRECTION, descend DIRECTION, sprint|sneak DIRECTION [ticks], crouch [ticks],\n\
     jump [direction] [ticks], center, wait [ticks|work|finish|boundary|until PHASE],\n\
     use [direction|at X Y Z], break [direction|at X Y Z],\n\
     break [direction|at X Y Z] repeat COUNT (terrain only, up to 32 cuts),\n\
     shove|strike|prepare|release [direction], cancel, slot N, select N,\n\
     empty, drop, respawn, save SLOT, quit.\n\
     Aim directly at a voxel centre with look/use/break at X Y Z; normal ray reach applies.\n\
     Directions for pointing: n, s, e, w, up, down, n-down, s-down, e-down, w-down,\n\
     n-shallow, s-shallow, e-shallow, w-shallow (aim downward),\n\
     n-up, s-up, e-up, w-up (aim at the side of raised framing).\n\
     Text play uses real 20 Hz control steps, never simulated turns."
}

fn parse_arguments() -> Result<Arguments, String> {
    parse_arguments_from(std::env::args().skip(1))
}

fn parse_arguments_from(args: impl IntoIterator<Item = String>) -> Result<Arguments, String> {
    let mut args = args.into_iter();
    let mut parsed = Arguments {
        seed: None,
        load: None,
        script: None,
        natural: false,
        random: false,
        habitat: None,
    };
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--seed" => {
                if parsed.seed.is_some() {
                    return Err("--seed can only be specified once".to_owned());
                }
                let value = args.next().ok_or("--seed needs a number")?;
                parsed.seed = Some(value.parse().map_err(|_| "invalid --seed number")?);
            }
            "--load" => {
                if parsed.load.is_some() {
                    return Err("--load can only be specified once".to_owned());
                }
                parsed.load = Some(args.next().ok_or("--load needs a save slot")?);
            }
            "--natural" => mark_once(&mut parsed.natural, "--natural")?,
            "--random" => mark_once(&mut parsed.random, "--random")?,
            "--habitat" => {
                set_habitat(&mut parsed.habitat, args.next())?;
            }
            "--script" => {
                if parsed.script.is_some() {
                    return Err("--script can only be specified once".to_owned());
                }
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
    validate_startup_sources(&parsed)?;
    Ok(parsed)
}

fn mark_once(used: &mut bool, name: &str) -> Result<(), String> {
    if std::mem::replace(used, true) {
        return Err(format!("{name} can only be specified once"));
    }
    Ok(())
}

fn set_habitat(slot: &mut Option<HabitatKind>, text: Option<String>) -> Result<(), String> {
    if slot.is_some() {
        return Err("--habitat can only be specified once".to_owned());
    }
    *slot = Some(match text.as_deref() {
        Some("dry") => HabitatKind::DryScrub,
        Some("mixed") => HabitatKind::MixedWoodland,
        Some("damp") => HabitatKind::DampGrove,
        _ => return Err("--habitat must be dry, mixed, or damp".to_owned()),
    });
    Ok(())
}

fn validate_startup_sources(parsed: &Arguments) -> Result<(), String> {
    if parsed.seed.is_some() && parsed.load.is_some() {
        return Err("--seed and --load cannot be used together".to_owned());
    }
    if parsed.load.is_some() && (parsed.natural || parsed.random) {
        return Err("world generation options cannot be combined with --load".to_owned());
    }
    if parsed.random && parsed.seed.is_some() {
        return Err(
            "--random generates its own seed; use --natural --seed NUMBER for replay".to_owned(),
        );
    }
    if parsed.random && parsed.natural {
        return Err("--random already uses the natural world; do not add --natural".to_owned());
    }
    if parsed.habitat.is_some() && !(parsed.natural || parsed.random) {
        return Err("--habitat requires --natural or --random".to_owned());
    }
    Ok(())
}

/// The only nondeterministic operation is choosing a new seed. HashMap's
/// randomly initialized state obtains operating-system seed entropy; printing
/// the seed makes the resulting world fully reproducible with --natural.
fn random_world_seed() -> u64 {
    let state = std::collections::hash_map::RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(0x4445_4550_5F48_4541);
    hasher.finish()
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
    let natural = args.natural || args.random;
    let chosen_seed = args.load.is_none().then(|| {
        args.seed
            .unwrap_or_else(|| if natural { random_world_seed() } else { 24_318 })
    });
    let mut session = if let Some(slot) = args.load {
        Session::load(&slot)?
    } else {
        let seed = chosen_seed.ok_or("missing seed for new wilderness")?;
        if natural {
            if args.habitat.is_some() {
                Session::new_natural_in_habitat(seed, args.habitat)?
            } else {
                Session::new_natural(seed)?
            }
        } else {
            Session::new(seed)?
        }
    };
    if natural {
        let seed = chosen_seed.ok_or("missing seed for natural wilderness")?;
        let habitat_flag = match args.habitat {
            Some(HabitatKind::DryScrub) => " --habitat dry",
            Some(HabitatKind::MixedWoodland) => " --habitat mixed",
            Some(HabitatKind::DampGrove) => " --habitat damp",
            None => "",
        };
        println!(
            "Natural wilderness | no authored starter trail | replay with --natural --seed {seed}{habitat_flag}"
        );
    }
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
