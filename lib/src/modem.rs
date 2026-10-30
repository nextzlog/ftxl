use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::message::Fields;

pub trait Modem {
    fn transmit(&mut self, text: &str, hz: f32, now: SystemTime) -> Result<Transmission, String>;
    fn push(&mut self, samples: &[f32], start: SystemTime) -> Vec<Decode>;
    fn options(&self) -> &Options;
    fn set_options(&mut self, options: Options);
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decode {
    pub slot: Option<SystemTime>,
    pub at: SystemTime,
    pub snr: f32,
    pub hz: f32,
    pub dt: Option<f32>,
    pub text: String,
    pub fields: Option<Fields>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    Even,
    Odd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transmission {
    pub audio: Vec<f32>,
    pub start: SystemTime,
    pub parity: Option<Parity>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub min_hz: f32,
    pub max_hz: f32,
    pub rx_hz: Option<f32>,
    pub ft: FtOptions,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FtOptions {
    pub candidates: usize,
    pub passes: usize,
    pub iterations: usize,
    pub threshold: f32,
    pub min_sync: usize,
    pub rx_width: f32,
    pub blanker: bool,
    pub fading: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            min_hz: 100.0,
            max_hz: 3000.0,
            rx_hz: None,
            ft: FtOptions::default(),
        }
    }
}

impl Default for FtOptions {
    fn default() -> Self {
        FtOptions {
            candidates: 300,
            passes: 3,
            iterations: 30,
            threshold: 1.3,
            min_sync: 8,
            rx_width: 100.0,
            blanker: true,
            fading: true,
        }
    }
}

pub const MAX_HZ: f32 = 5500.0;

impl Options {
    pub fn check(&self) -> Result<(), String> {
        let ok = |c: bool, what: &str| if c { Ok(()) } else { Err(what.to_string()) };
        ok(self.min_hz.is_finite() && self.min_hz >= 0.0, "min_hz")?;
        ok(
            self.max_hz.is_finite() && self.max_hz > self.min_hz && self.max_hz <= MAX_HZ,
            "max_hz",
        )?;
        ok(self.rx_hz.is_none_or(|f| f.is_finite()), "rx_hz")?;
        let f = &self.ft;
        ok(f.candidates > 0 && f.passes > 0 && f.iterations > 0, "ft")?;
        ok(f.threshold.is_finite() && f.rx_width.is_finite(), "ft")
    }

    pub fn sanitized(self) -> Options {
        let d = Options::default();
        let fin = |v: f32, or: f32| if v.is_finite() { v } else { or };
        let min_hz = fin(self.min_hz, d.min_hz).clamp(0.0, MAX_HZ - 1.0);
        let max_hz = fin(self.max_hz, d.max_hz).clamp(min_hz + 1.0, MAX_HZ);
        let f = self.ft;
        Options {
            min_hz,
            max_hz,
            rx_hz: self.rx_hz.filter(|v| v.is_finite()),
            ft: FtOptions {
                candidates: f.candidates.clamp(1, 5000),
                passes: f.passes.clamp(1, 10),
                iterations: f.iterations.clamp(1, 1000),
                threshold: fin(f.threshold, d.ft.threshold),
                min_sync: f.min_sync,
                rx_width: fin(f.rx_width, d.ft.rx_width).abs(),
                blanker: f.blanker,
                fading: f.fading,
            },
        }
    }
}

pub(crate) fn nanos(t: SystemTime) -> i128 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    }
}

pub(crate) fn time(nanos: i128) -> SystemTime {
    let d = |n: u128| Duration::new((n / 1_000_000_000) as u64, (n % 1_000_000_000) as u32);
    if nanos >= 0 {
        UNIX_EPOCH + d(nanos as u128)
    } else {
        UNIX_EPOCH - d(nanos.unsigned_abs())
    }
}

pub(crate) fn finite(samples: &[f32]) -> std::borrow::Cow<'_, [f32]> {
    if samples.iter().all(|x| x.is_finite()) {
        samples.into()
    } else {
        samples
            .iter()
            .map(|&x| if x.is_finite() { x } else { 0.0 })
            .collect::<Vec<_>>()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_checked_and_sanitized() {
        assert_eq!(Options::default().check(), Ok(()));
        let bad = [
            Options {
                min_hz: -1.0,
                ..Options::default()
            },
            Options {
                max_hz: 50.0,
                ..Options::default()
            },
            Options {
                max_hz: 9000.0,
                ..Options::default()
            },
            Options {
                min_hz: f32::NAN,
                ..Options::default()
            },
            Options {
                rx_hz: Some(f32::INFINITY),
                ..Options::default()
            },
            Options {
                ft: FtOptions {
                    passes: 0,
                    ..FtOptions::default()
                },
                ..Options::default()
            },
        ];
        for o in bad {
            assert!(o.check().is_err(), "{o:?}");
            assert_eq!(o.sanitized().check(), Ok(()), "{o:?}");
        }
        assert_eq!(Options::default().sanitized(), Options::default());
    }

    #[test]
    fn time_conversion_is_exact_around_the_epoch_and_far_away() {
        for n in [
            0i128,
            1,
            -1,
            999_999_999,
            -1_000_000_001,
            1_800_000_000_123_456_789,
            40_000_000_000_000_000_000,
            -40_000_000_000_000_000_000,
        ] {
            assert_eq!(nanos(time(n)), n);
        }
    }

    #[test]
    fn non_finite_samples_become_silence() {
        assert!(matches!(finite(&[1.0, 2.0]), std::borrow::Cow::Borrowed(_)));
        assert_eq!(
            &*finite(&[f32::NAN, 1.0, f32::NEG_INFINITY]),
            &[0.0, 1.0, 0.0]
        );
    }
}
