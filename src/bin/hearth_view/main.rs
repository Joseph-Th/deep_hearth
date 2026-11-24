//! Desktop block-world viewer adapter.
//!
//! Creates a natural world through production generation operations, meshes it with the
//! renderer-neutral `meshing` owner, and draws it with the authored WGSL shader set. The viewer
//! never mutates simulation state; it only chooses a display tick for daylight and seasons.

#![forbid(unsafe_code)]

mod args;
mod camera;
mod capture;
mod daylight;
mod pipelines;
mod renderer;
mod scene;
mod window;

use std::process::ExitCode;

use glam::Vec3;

use args::{Options, Parsed};
use camera::Camera;
use daylight::Daylight;
use renderer::FrameInput;
use scene::Scene;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("hearth-view: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = match args::parse(std::env::args().skip(1))? {
        Parsed::Run(options) => options,
        Parsed::Help => {
            println!("{}", args::USAGE);
            return Ok(());
        }
    };
    let scene = Scene::generate(&options)?;
    let camera = initial_camera(&scene, &options);
    match &options.screenshot {
        Some(path) => {
            let daylight = Daylight::at(scene.day_fraction());
            let input = FrameInput {
                camera: &camera,
                daylight: &daylight,
                time_seconds: 0.0,
                frame_index: 0,
            };
            capture::render_to_file(&scene, &input, (options.width, options.height), path)?;
            eprintln!("wrote {}", path.display());
            Ok(())
        }
        None => window::run(scene, camera),
    }
}

fn initial_camera(scene: &Scene, options: &Options) -> Camera {
    let eye = Vec3::from(scene.spawn_eye());
    Camera {
        position: eye + Vec3::Y * options.rise,
        yaw: options.yaw_degrees.to_radians(),
        pitch: options.pitch_degrees.to_radians(),
    }
}
