//! Console front end for stage 2: phone camera → "Plugcam Camera" without the GUI.
//!
//! While running, type a letter and Enter: t = torch, + / - = zoom, m = mirror, q = quit.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;

use plugcam::adb::Adb;
use plugcam::pipeline::{Pipeline, PipelineConfig};
use plugcam::resources;
use plugcam::scrcpy::protocol::ControlMessage;
use plugcam::scrcpy::server::{self, CameraParams, Facing};
use plugcam::vcam::VirtualCamera;

const USAGE: &str = "\
usage: plugcam-cli [options]
  --list                 list the phone's cameras and sizes, then exit
  --serial SERIAL        phone to use (default: first one, USB first)
  --camera-id ID         camera id from --list (overrides --facing)
  --facing back|front|external   (default: back)
  --size WxH             capture size (default: 1920x1080)
  --fps N                capture fps (default: 30)
  --high-speed           constrained high-speed mode (use with --fps 120)
  --bitrate MBPS         video bit rate in Mbit/s (default: scrcpy's 8)
  --orientation DEG      0, 90, 180 or 270, rotated on the phone
  --zoom X               initial zoom
  --mirror               mirror the picture
  --vcam-size WxH        virtual camera size (default: 1920x1080)
  --dump FILE            also save the raw video stream (test fixtures)
  --verbose              debug logging";

struct Args {
    list: bool,
    serial: Option<String>,
    camera: CameraParams,
    mirror: bool,
    vcam_size: (u32, u32),
    dump: Option<PathBuf>,
    verbose: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        list: false,
        serial: None,
        camera: CameraParams::default(),
        mirror: false,
        vcam_size: (1920, 1080),
        dump: None,
        verbose: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--list" => a.list = true,
            "--serial" => a.serial = Some(value()?),
            "--camera-id" => a.camera.camera_id = Some(value()?),
            "--facing" => {
                a.camera.facing = Some(match value()?.as_str() {
                    "back" => Facing::Back,
                    "front" => Facing::Front,
                    "external" => Facing::External,
                    other => return Err(format!("unknown facing {other}")),
                })
            }
            "--size" => a.camera.size = Some(parse_size(&value()?)?),
            "--fps" => a.camera.fps = value()?.parse().map_err(|_| "bad --fps")?,
            "--high-speed" => a.camera.high_speed = true,
            "--bitrate" => {
                let mbps: f32 = value()?.parse().map_err(|_| "bad --bitrate")?;
                a.camera.bit_rate = Some((mbps * 1_000_000.0) as u32);
            }
            "--orientation" => {
                a.camera.orientation = match value()?.as_str() {
                    "0" => 0,
                    "90" => 90,
                    "180" => 180,
                    "270" => 270,
                    _ => return Err("--orientation must be 0, 90, 180 or 270".into()),
                }
            }
            "--zoom" => a.camera.zoom = Some(value()?.parse().map_err(|_| "bad --zoom")?),
            "--mirror" => a.mirror = true,
            "--vcam-size" => a.vcam_size = parse_size(&value()?)?,
            "--dump" => a.dump = Some(value()?.into()),
            "--verbose" => a.verbose = true,
            "-h" | "--help" => return Err(USAGE.into()),
            other => return Err(format!("unknown option {other}\n{USAGE}")),
        }
    }
    Ok(a)
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once('x').ok_or_else(|| format!("size must look like 1920x1080, got {s}"))?;
    Ok((w.parse().map_err(|_| format!("bad width in {s}"))?, h.parse().map_err(|_| format!("bad height in {s}"))?))
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    env_logger::Builder::new()
        .filter_level(if args.verbose { log::LevelFilter::Debug } else { log::LevelFilter::Info })
        .format_timestamp_millis()
        .init();

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), String> {
    let adb = Adb::locate().map_err(|e| e.to_string())?;
    let server_file = server::server_file().map_err(|e| e.to_string())?;
    log::info!("adb: {}, server: {}", adb.exe().display(), server_file.display());

    if args.list {
        let devices = adb.devices().map_err(|e| e.to_string())?;
        let device = devices
            .iter()
            .find(|d| d.is_ready() && args.serial.as_ref().is_none_or(|s| s == &d.serial))
            .ok_or("no phone connected")?;
        print!("{}", server::list_cameras(&adb, &device.serial, &server_file).map_err(|e| e.to_string())?);
        return Ok(());
    }

    let dll = resources::vcam_dll().ok_or("plugcam_cam.dll not found (build vcam or set PLUGCAM_VCAM_DLL)")?;
    let vcam = VirtualCamera::create(&dll, args.vcam_size.0, args.vcam_size.1).map_err(|e| e.to_string())?;
    log::info!("Plugcam Camera is up at {}x{}", vcam.width(), vcam.height());

    let mut mirror = args.mirror;
    let pipeline = Pipeline::start(
        PipelineConfig {
            adb,
            server_file,
            serial: args.serial,
            camera: args.camera.clone(),
            mirror,
            dump: args.dump,
        },
        vcam,
        |_| {},
    );

    println!("commands: t = torch, + / - = zoom, m = mirror, q = quit");
    let mut torch = args.camera.torch;
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let result = match line.trim() {
            "q" => break,
            "t" => {
                torch = !torch;
                pipeline.send_control(ControlMessage::SetTorch(torch))
            }
            "+" => pipeline.send_control(ControlMessage::ZoomIn),
            "-" => pipeline.send_control(ControlMessage::ZoomOut),
            "m" => {
                mirror = !mirror;
                pipeline.set_mirror(mirror);
                Ok(())
            }
            "" => Ok(()),
            other => {
                println!("unknown command {other:?}");
                Ok(())
            }
        };
        if let Err(e) = result {
            log::warn!("{e}");
        }
    }
    pipeline.stop();
    Ok(())
}
