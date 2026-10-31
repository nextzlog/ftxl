use std::time::Duration;

use ftxl::sim::{ft_trial, render, Agc, Band, Preset, Qrn, Source, Station, Trial};
use ftxl::{Ft4, Ft8, Modem, Options};

const USAGE: &str = "usage: bench <ft8|ft4> [preset|qrn|crowd|silence|all] [trials] [plain]";

fn presets(name: &str) -> Vec<(String, Preset, Qrn)> {
    let qrn = Qrn {
        rate: 4.0,
        level: 60.0,
    };
    match name {
        "all" => Preset::ALL
            .iter()
            .map(|p| (p.name().to_string(), *p, Qrn::default()))
            .chain([("qrn".into(), Preset::Awgn, qrn)])
            .collect(),
        "qrn" => vec![("qrn".into(), Preset::Awgn, qrn)],
        "crowd" => vec![("crowd".into(), Preset::Awgn, Qrn::default())],
        _ => Preset::ALL
            .iter()
            .filter(|p| p.name() == name)
            .map(|p| (p.name().to_string(), *p, Qrn::default()))
            .collect(),
    }
}

fn options(plain: bool) -> Options {
    let mut o = Options::default();
    if plain {
        o.ft.blanker = false;
        o.ft.fading = false;
    }
    o
}

fn ms(d: Duration, n: u64) -> f64 {
    d.as_secs_f64() * 1e3 / n.max(1) as f64
}

fn ft(source: Source, name: &str, crowd: bool, preset: Preset, qrn: Qrn, trials: u64, plain: bool) {
    let mut modem: Box<dyn Modem> = match source {
        Source::Ft4 => Box::new(Ft4::new(12000, options(plain)).unwrap()),
        _ => Box::new(Ft8::new(12000, options(plain)).unwrap()),
    };
    let band = Band {
        qrn,
        ..Band::slot(source)
    };
    println!("{source:?} {name}: snr_db decoded/sent false mean_ms");
    for snr in (-26..=-4).rev().step_by(2) {
        let mut sum = Trial::default();
        for seed in 0..trials {
            let n = if crowd { 20 } else { 1 };
            let stations: Vec<Station> = (0..n)
                .map(|i| {
                    let call = format!("JA{}{}", i % 10, ["ZLO", "YWX", "XYZ"][i / 10 % 3]);
                    let hz = 300.0 + 2400.0 * (i as f32 + (0.37 * seed as f32).fract()) / n as f32;
                    Station::new(
                        &format!("CQ {call} PM95"),
                        hz,
                        0.1 * (i % 5) as f32 - 0.2,
                        snr as f32,
                    )
                    .with(preset.channel())
                })
                .collect();
            sum += ft_trial(&mut *modem, source, &band, &stations, 1000 + seed).unwrap();
        }
        println!(
            "{snr:7} {:4}/{:<4} {:5} {:8.1}",
            sum.decoded,
            sum.sent,
            sum.false_decodes,
            ms(sum.elapsed, trials)
        );
    }
}

fn empty(seconds: f32) -> Vec<(&'static str, Band, Vec<Station>)> {
    let base = Band::audio(12000, seconds);
    let crashes = Band {
        qrn: Qrn {
            rate: 3.0,
            level: 80.0,
        },
        ..base
    };
    let carriers = [1000.0, 1503.0, 2125.0, 2295.0]
        .map(|hz| Station::new("", hz, 0.0, 5.0))
        .to_vec();
    vec![
        ("white", base, vec![]),
        ("crashes", crashes, vec![]),
        (
            "breathing",
            Band {
                agc: Agc {
                    depth_db: 15.0,
                    hz: 0.3,
                },
                ..base
            },
            vec![],
        ),
        ("carriers", crashes, carriers),
    ]
}

fn silence(mode: &str, minutes: u64) {
    let seconds = 60.0;
    for (name, band, stations) in empty(seconds) {
        {
            let source = if mode == "ft4" {
                Source::Ft4
            } else {
                Source::Ft8
            };
            let mut m: Box<dyn Modem> = match source {
                Source::Ft4 => Box::new(Ft4::new(12000, Options::default()).unwrap()),
                _ => Box::new(Ft8::new(12000, Options::default()).unwrap()),
            };
            let mut false_decodes = 0;
            for seed in 0..minutes {
                let x = render(&band, Source::Tone, &stations, seed).unwrap();
                false_decodes += m.push(&x, std::time::UNIX_EPOCH).len();
            }
            let hours = minutes as f64 / 60.0;
            println!(
                "{source:?} silence {name}: false_decodes_per_hour {:.2} over {:.2} h",
                false_decodes as f64 / hours,
                hours
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("ft8");
    let name = args.get(1).map(String::as_str).unwrap_or("awgn");
    let trials = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let plain = args.iter().any(|a| a == "plain");
    if name == "silence" {
        return silence(mode, trials);
    }
    let list = presets(name);
    if list.is_empty() {
        eprintln!("{USAGE}");
    }
    for (name, preset, qrn) in list {
        match mode {
            "ft8" => ft(
                Source::Ft8,
                &name,
                name == "crowd",
                preset,
                qrn,
                trials,
                plain,
            ),
            "ft4" => ft(
                Source::Ft4,
                &name,
                name == "crowd",
                preset,
                qrn,
                trials,
                plain,
            ),
            _ => eprintln!("{USAGE}"),
        }
    }
}
