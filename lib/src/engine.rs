use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::modem::{Decode, Modem, Options, Parity};
use crate::sequence::{Contact, Sequencer};

const LEAD: Duration = Duration::from_millis(100);

const LATE: Duration = Duration::from_millis(1000);

const DELAY: Duration = Duration::from_millis(500);

const FT8_DIALS: [u64; 10] = [
    3_573_000,
    7_074_000,
    10_136_000,
    14_074_000,
    18_100_000,
    21_074_000,
    24_915_000,
    28_074_000,
    50_313_000,
    144_174_000,
];

const FT4_DIALS: [u64; 10] = [
    3_575_000,
    7_047_500,
    10_140_000,
    14_080_000,
    18_104_000,
    21_140_000,
    24_919_000,
    28_180_000,
    50_318_000,
    144_170_000,
];

const SKEW: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Proto {
    #[default]
    Ft8,
    Ft4,
}

impl Proto {
    pub const ALL: [Proto; 2] = [Proto::Ft8, Proto::Ft4];

    pub fn id(self) -> &'static str {
        match self {
            Proto::Ft8 => "ft8",
            Proto::Ft4 => "ft4",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Proto::Ft8 => "FT8",
            Proto::Ft4 => "FT4",
        }
    }

    pub fn dials(self) -> &'static [u64] {
        match self {
            Proto::Ft8 => &FT8_DIALS,
            Proto::Ft4 => &FT4_DIALS,
        }
    }

    pub fn period(self) -> Duration {
        match self {
            Proto::Ft8 => Duration::from_secs(15),
            Proto::Ft4 => Duration::from_millis(7500),
        }
    }

    pub fn open(self, rate: u32, o: Options) -> Result<Box<dyn Modem + Send>, String> {
        Ok(match self {
            Proto::Ft8 => Box::new(crate::Ft8::new(rate, o)?),
            Proto::Ft4 => Box::new(crate::Ft4::new(rate, o)?),
        })
    }
}

pub fn next_slot(now: SystemTime, period: Duration, parity: Parity) -> SystemTime {
    let p = period.as_nanos();
    let t = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut k = t.div_ceil(p);
    let at = |k: u128| UNIX_EPOCH + Duration::from_nanos((k * p) as u64);
    if Parity::of(at(k), period) != parity {
        k += 1;
    }
    at(k)
}

pub fn slot_start(now: SystemTime, period: Duration) -> SystemTime {
    let p = period.as_nanos();
    let t = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    UNIX_EPOCH + Duration::from_nanos((t / p * p) as u64)
}

fn other(p: Parity) -> Parity {
    match p {
        Parity::Even => Parity::Odd,
        Parity::Odd => Parity::Even,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Burst {
    pub at: SystemTime,
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Burst {
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / self.rate.max(1) as f64)
    }

    pub fn end(&self) -> SystemTime {
        self.at + self.duration()
    }

    pub fn lead_in(&self, now: SystemTime) -> Vec<f32> {
        let wait = self.at.duration_since(now).unwrap_or_default();
        let n = (wait.as_secs_f64() * self.rate as f64).round() as usize;
        let mut audio = vec![0.0; n];
        audio.extend_from_slice(&self.samples);
        audio
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub callsign: String,
    pub grid: Option<String>,
    pub hz: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Qso {
    pub mode: Proto,
    pub callsign: String,
    pub grid: String,
    pub sent: String,
    pub rcvd: String,
    pub hz: f32,
    pub dial_hz: Option<u64>,
    pub at: SystemTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub mode: Proto,
    pub callsign: String,
    pub grid: Option<String>,
    pub snr: f32,
    pub hz: f32,
    pub dial_hz: Option<u64>,
    pub at: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Warning {
    ClockSkew(f32),
    Latency(Duration),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Setup {
    pub proto: Proto,
    pub rx_hz: f32,
    pub tx_hz: f32,
    pub parity: Parity,
    pub options: Options,
    pub seq: Sequencer,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            proto: Proto::Ft8,
            rx_hz: 1500.0,
            tx_hz: 1500.0,
            parity: Parity::Even,
            options: Options::default(),
            seq: Sequencer::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Start,
    Stop,
    Halt,
    Setup(Box<Setup>),
    Select(Box<Decode>),
    Samples {
        rate: u32,
        samples: Vec<f32>,
        at: SystemTime,
    },
    OutputRate(u32),
    Tick(SystemTime),
    PttLead(Option<Duration>),
    Keyed(bool),
    KeyFailed(String),
    Dial(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Running(bool),
    Setup(Box<Setup>),
    Decode(Decode),
    Spot(Spot),
    Pick(Pick),
    QsoCompleted(Qso),
    Burst(Burst),
    Halt,
    Ptt(bool),
    Tune(u64),
    Warnings(Vec<Warning>),
    Error(String),
}

struct Rx {
    rate: u32,
    proto: Proto,
    modem: Box<dyn Modem + Send>,
}

pub struct Engine {
    pub latency: Option<Duration>,
    setup: Setup,
    told: Setup,
    rx: Option<Rx>,
    tx: Option<Rx>,
    out_rate: u32,
    running: bool,
    due: Option<SystemTime>,
    tx_end: Option<SystemTime>,
    pending: Option<Burst>,
    lead: Option<Duration>,
    dial: Option<u64>,
    ptt: bool,
    warnings: Vec<Warning>,
    out: Vec<Notice>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            latency: None,
            setup: Setup::default(),
            told: Setup::default(),
            rx: None,
            tx: None,
            out_rate: 48000,
            running: false,
            due: None,
            tx_end: None,
            pending: None,
            lead: None,
            dial: None,
            ptt: false,
            warnings: Vec::new(),
            out: Vec::new(),
        }
    }

    pub fn setup(&self) -> &Setup {
        &self.setup
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn sending(&self) -> bool {
        self.ptt
    }

    pub fn take(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.out)
    }

    pub fn handle(&mut self, c: Command) {
        match c {
            Command::Start => self.start(),
            Command::Stop => self.stop(),
            Command::Halt => self.halt(),
            Command::Setup(s) => {
                let mut s = *s;
                s.options = s.options.sanitized();
                s.rx_hz = s.rx_hz.clamp(0.0, s.options.max_hz);
                s.tx_hz = s.tx_hz.clamp(0.0, s.options.max_hz);
                self.retune(s.proto);
                self.told = s.clone();
                self.setup = s;
            }
            Command::Select(d) => self.select(&d),
            Command::Samples { rate, samples, at } => self.feed(rate, &samples, at),
            Command::OutputRate(rate) => self.out_rate = rate,
            Command::Tick(now) => self.tick(now),
            Command::PttLead(lead) => self.lead = lead,
            Command::Keyed(on) => self.keyed(on),
            Command::KeyFailed(e) => {
                if self.ptt {
                    self.halt();
                }
                self.out.push(Notice::Error(e));
            }
            Command::Dial(hz) => self.dial = Some(hz),
        }
        if self.setup != self.told {
            self.told = self.setup.clone();
            self.out.push(Notice::Setup(Box::new(self.setup.clone())));
        }
    }

    fn start(&mut self) {
        if !self.running {
            self.running = true;
            self.out.push(Notice::Running(true));
        }
    }

    fn stop(&mut self) {
        self.halt();
        if self.running {
            self.running = false;
            self.rx = None;
            self.out.push(Notice::Running(false));
        }
    }

    fn halt(&mut self) {
        self.setup.seq.halt();
        self.due = None;
        self.pending = None;
        if self.tx_end.take().is_some() {
            self.out.push(Notice::Halt);
        }
        self.set_ptt(false);
    }

    fn retune(&mut self, to: Proto) {
        let from = self.setup.proto;
        let band = self
            .dial
            .and_then(|d| from.dials().iter().position(|f| *f == d));
        if let Some(i) = band.filter(|_| from != to) {
            self.out.push(Notice::Tune(to.dials()[i]));
        }
    }

    fn keyed(&mut self, on: bool) {
        if !on {
            return;
        }
        if let Some(b) = self.pending.take() {
            self.tx_end = Some(b.end());
            self.out.push(Notice::Burst(b));
        }
    }

    fn set_ptt(&mut self, on: bool) {
        if self.ptt != on {
            self.ptt = on;
            self.out.push(Notice::Ptt(on));
        }
    }

    fn options(&self) -> Options {
        Options {
            rx_hz: Some(self.setup.rx_hz),
            ..self.setup.options
        }
    }

    fn modem(
        slot: &mut Option<Rx>,
        rate: u32,
        proto: Proto,
        o: Options,
    ) -> Result<&mut Rx, String> {
        if !slot
            .as_ref()
            .is_some_and(|r| r.rate == rate && r.proto == proto)
        {
            *slot = Some(Rx {
                rate,
                proto,
                modem: proto.open(rate, o)?,
            });
        }
        let r = slot.as_mut().expect("opened");
        if *r.modem.options() != o {
            r.modem.set_options(o);
        }
        Ok(r)
    }

    fn feed(&mut self, rate: u32, samples: &[f32], at: SystemTime) {
        if !self.running {
            return;
        }
        let (o, proto) = (self.options(), self.setup.proto);
        let clock = Instant::now();
        let decodes = match Self::modem(&mut self.rx, rate, proto, o) {
            Ok(r) => r.modem.push(samples, at),
            Err(e) => {
                self.out.push(Notice::Error(e));
                return;
            }
        };
        if decodes.is_empty() {
            return;
        }
        self.warn(&decodes, clock.elapsed());
        for d in decodes {
            if let Some(c) = self.setup.seq.heard(&d) {
                let q = self.qso(c, d.at);
                self.out.push(Notice::QsoCompleted(q));
            }
            if let Some(f) = d.fields.as_ref().filter(|f| !f.de.is_empty()) {
                let grid = f.extra.strip_prefix("R ").unwrap_or(&f.extra);
                let grid = (grid.len() == 4 && grid != "RR73" && !grid.starts_with(['+', '-']))
                    .then(|| grid.to_string());
                self.out.push(Notice::Spot(Spot {
                    mode: proto,
                    callsign: f.de.clone(),
                    grid,
                    snr: d.snr,
                    hz: d.hz,
                    dial_hz: self.dial,
                    at: d.at,
                }));
            }
            self.out.push(Notice::Decode(d));
        }
    }

    fn warn(&mut self, decodes: &[Decode], elapsed: Duration) {
        let mut dt: Vec<f32> = decodes.iter().filter_map(|d| d.dt).collect();
        dt.sort_by(f32::total_cmp);
        let mut w = Vec::new();
        if let Some(&m) = dt.get(dt.len() / 2) {
            if m.abs() > SKEW {
                w.push(Warning::ClockSkew((m * 10.0).round() / 10.0));
            }
        }
        let limit = self.latency.unwrap_or(self.setup.proto.period() / 10);
        if elapsed > limit {
            w.push(Warning::Latency(elapsed));
        }
        if w != self.warnings || !w.is_empty() {
            self.warnings = w.clone();
            self.out.push(Notice::Warnings(w));
        }
    }

    fn qso(&self, c: Contact, at: SystemTime) -> Qso {
        Qso {
            mode: self.setup.proto,
            callsign: c.call,
            grid: c.grid,
            sent: c.sent,
            rcvd: c.rcvd,
            hz: self.setup.tx_hz,
            dial_hz: self.dial,
            at,
        }
    }

    fn select(&mut self, d: &Decode) {
        let s = &mut self.setup;
        s.rx_hz = d.hz;
        if let Some(slot) = d.slot {
            s.parity = other(Parity::of(slot, s.proto.period()));
        }
        let done = s.seq.select(d);
        if let Some(f) = d.fields.as_ref().filter(|f| s.seq.his_call == f.de) {
            let grid = Some(s.seq.his_grid.clone()).filter(|g| !g.is_empty());
            self.out.push(Notice::Pick(Pick {
                callsign: f.de.clone(),
                grid,
                hz: d.hz,
            }));
        }
        if let Some(c) = done {
            let q = self.qso(c, d.at);
            self.out.push(Notice::QsoCompleted(q));
        }
    }

    fn tick(&mut self, now: SystemTime) {
        if let Some(b) = &self.pending {
            if now > b.at {
                self.pending = None;
                self.set_ptt(false);
                self.out
                    .push(Notice::Error("PTT not confirmed, slot skipped".into()));
            }
            return;
        }
        if let Some(end) = self.tx_end {
            if now >= end {
                self.tx_end = None;
                self.setup.seq.sent();
                self.set_ptt(false);
            }
            return;
        }
        if !self.running || !self.setup.seq.enabled {
            self.due = None;
            return;
        }
        let period = self.setup.proto.period();
        let parity = self.setup.parity;
        let due = self
            .due
            .filter(|d| Parity::of(*d, period) == parity && *d + LATE >= now)
            .unwrap_or_else(|| next_slot(now, period, parity));
        self.due = Some(due);
        let ahead = self
            .lead
            .map_or(LEAD, |l| LEAD.max(l.saturating_sub(DELAY)));
        if now + ahead < due {
            return;
        }
        self.due = None;
        if self.lead.is_some() && now > due + DELAY {
            return;
        }
        let text = self.setup.seq.next();
        let (rate, proto, o, hz) = (
            self.out_rate,
            self.setup.proto,
            self.options(),
            self.setup.tx_hz,
        );
        let tx = Self::modem(&mut self.tx, rate, proto, o)
            .and_then(|m| m.modem.transmit(&text, hz, due));
        match tx {
            Ok(tx) => {
                let b = Burst {
                    at: tx.start,
                    rate,
                    samples: tx.audio,
                };
                self.set_ptt(true);
                if self.lead.is_some() {
                    self.pending = Some(b);
                } else {
                    self.tx_end = Some(b.end());
                    self.out.push(Notice::Burst(b));
                }
            }
            Err(e) => {
                self.setup.seq.halt();
                self.out.push(Notice::Error(e));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::tests::decode;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_793_000_000 / 30 * 30 + secs)
    }

    fn engine() -> Engine {
        let mut e = Engine::new();
        let mut s = Setup::default();
        s.seq.my_call = "JA1ZLO".into();
        s.seq.my_grid = "PM95".into();
        e.handle(Command::Setup(Box::new(s)));
        e.handle(Command::OutputRate(12000));
        e.handle(Command::Start);
        e.take();
        e
    }

    fn enable(e: &mut Engine, f: impl FnOnce(&mut Setup)) {
        let mut s = e.setup().clone();
        s.seq.enabled = true;
        f(&mut s);
        e.handle(Command::Setup(Box::new(s)));
    }

    #[test]
    fn slots_of_either_parity_follow_now() {
        let p = Duration::from_secs(15);
        assert_eq!(next_slot(at(1), p, Parity::Odd), at(15));
        assert_eq!(next_slot(at(1), p, Parity::Even), at(30));
        assert_eq!(next_slot(at(0), p, Parity::Even), at(0));
        assert_eq!(slot_start(at(29), p), at(15));
        let p4 = Duration::from_millis(7500);
        assert_eq!(
            next_slot(at(1), p4, Parity::Odd),
            at(0) + Duration::from_millis(7500)
        );
    }

    #[test]
    fn a_burst_waits_with_silence_and_ends_after_its_samples() {
        let at = UNIX_EPOCH + Duration::from_millis(15500);
        let b = Burst {
            at,
            rate: 12000,
            samples: vec![1.0; 12000],
        };
        let audio = b.lead_in(at - Duration::from_millis(600));
        assert_eq!(audio.len(), 7200 + 12000);
        assert!(audio[..7200].iter().all(|v| *v == 0.0) && audio[7200] == 1.0);
        assert_eq!(b.lead_in(at + Duration::from_secs(1)).len(), 12000);
        assert_eq!(b.end(), at + Duration::from_secs(1));
    }

    #[test]
    fn a_selected_report_to_me_completes_the_qso_and_picks_the_station() {
        let mut e = engine();
        let mut d = decode("JA1ZLO JA1YWX R-07", -12.0);
        d.slot = Some(at(15));
        e.handle(Command::Select(Box::new(d)));
        let n = e.take();
        assert!(matches!(&n[0], Notice::Pick(k) if k.callsign == "JA1YWX" && k.hz == 900.0));
        let Some(Notice::QsoCompleted(q)) = n.get(1) else {
            panic!("{n:?}")
        };
        assert_eq!(
            (q.callsign.as_str(), q.mode, q.rcvd.as_str()),
            ("JA1YWX", Proto::Ft8, "-07")
        );
        let Some(Notice::Setup(s)) = n.last() else {
            panic!("{n:?}")
        };
        assert_eq!((s.parity, s.rx_hz), (Parity::Even, 900.0));
        assert_eq!(s.seq.next(), "JA1YWX JA1ZLO RR73");
    }

    #[test]
    fn a_stale_or_flipped_slot_is_not_sent_and_halt_unkeys() {
        let mut e = engine();
        enable(&mut e, |s| s.parity = Parity::Even);
        e.due = Some(at(0));
        e.handle(Command::Tick(at(29)));
        assert!(!e.sending(), "a slot left from long ago");
        assert_eq!(e.due, Some(at(30)));
        enable(&mut e, |s| s.parity = Parity::Odd);
        e.handle(Command::Tick(at(30)));
        assert!(!e.sending(), "the slot of the other parity");
        e.handle(Command::Tick(at(44) + Duration::from_millis(950)));
        assert!(e.sending());
        let n = e.take();
        let Some(Notice::Burst(b)) = n.last() else {
            panic!("{n:?}")
        };
        assert_eq!(b.at, at(45) + Duration::from_millis(500));
        assert_eq!(n[0], Notice::Ptt(true));
        e.handle(Command::Halt);
        let n = e.take();
        assert_eq!(n[..2], [Notice::Halt, Notice::Ptt(false)]);
        assert!(!e.setup().seq.enabled);
    }

    #[test]
    fn ptt_falls_after_the_burst_and_tx5_ends_the_sequence() {
        let mut e = engine();
        enable(&mut e, |s| {
            s.seq.tx = 5;
            s.seq.his_call = "JA1YWX".into();
        });
        e.handle(Command::Tick(at(29) + Duration::from_millis(950)));
        let end = e.tx_end.unwrap();
        e.handle(Command::Tick(end - Duration::from_millis(1)));
        assert!(e.sending());
        e.handle(Command::Tick(end));
        assert!(!e.sending() && !e.setup().seq.enabled);
        let n = e.take();
        assert!(n.contains(&Notice::Ptt(false)));
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn keyer(proto: Proto, lead: u64) -> Engine {
        let mut e = engine();
        e.handle(Command::PttLead(Some(ms(lead))));
        enable(&mut e, |s| {
            s.proto = proto;
            s.parity = Parity::Odd;
        });
        e.take();
        e
    }

    #[test]
    fn ptt_is_requested_a_lead_ahead_and_audio_waits_for_the_keyer() {
        for (proto, slot) in [(Proto::Ft8, at(15)), (Proto::Ft4, at(0) + ms(7500))] {
            let start = slot + ms(500);
            let mut e = keyer(proto, 800);
            e.handle(Command::Tick(start - ms(801)));
            assert!(e.take().is_empty());
            e.handle(Command::Tick(start - ms(800)));
            assert_eq!(e.take(), [Notice::Ptt(true)]);
            e.handle(Command::Tick(start - ms(10)));
            assert!(e.take().is_empty(), "no audio before the keyer confirms");
            e.handle(Command::Keyed(true));
            let n = e.take();
            let [Notice::Burst(b)] = &n[..] else {
                panic!("{n:?}")
            };
            assert_eq!(b.at, start);
            e.handle(Command::Tick(b.end()));
            assert_eq!(e.take().last(), Some(&Notice::Ptt(false)));
        }
    }

    #[test]
    fn a_short_lead_keeps_the_usual_preparation() {
        let mut e = keyer(Proto::Ft8, 50);
        e.handle(Command::Tick(at(15) - ms(100)));
        assert_eq!(e.take(), [Notice::Ptt(true)]);
        e.handle(Command::Tick(at(15) + ms(500)));
        e.handle(Command::Keyed(true));
        assert!(matches!(&e.take()[..], [Notice::Burst(b)] if b.at == at(15) + ms(500)));
    }

    #[test]
    fn a_late_confirmation_skips_the_slot_and_the_next_one_is_tried() {
        let mut e = keyer(Proto::Ft8, 300);
        e.handle(Command::Tick(at(15) - ms(100)));
        e.handle(Command::Tick(at(15) + ms(501)));
        let n = e.take();
        assert_eq!(n[..2], [Notice::Ptt(true), Notice::Ptt(false)]);
        assert!(matches!(n[2], Notice::Error(_)));
        e.handle(Command::Keyed(true));
        assert!(e.take().is_empty() && e.setup().seq.enabled);
        e.handle(Command::Tick(at(16)));
        assert!(e.take().is_empty(), "too late for this slot");
        e.handle(Command::Tick(at(45) - ms(100)));
        assert_eq!(e.take(), [Notice::Ptt(true)]);
    }

    #[test]
    fn halt_and_key_failure_drop_ptt_at_once() {
        let mut e = keyer(Proto::Ft8, 300);
        e.handle(Command::Tick(at(15) - ms(100)));
        e.take();
        e.handle(Command::Halt);
        assert_eq!(e.take()[0], Notice::Ptt(false));
        e.handle(Command::Keyed(true));
        assert!(e.take().is_empty());
        let mut e = keyer(Proto::Ft8, 300);
        e.handle(Command::Tick(at(15) - ms(100)));
        e.handle(Command::Keyed(true));
        e.take();
        e.handle(Command::Halt);
        assert_eq!(e.take()[..2], [Notice::Halt, Notice::Ptt(false)]);
        let mut e = keyer(Proto::Ft8, 300);
        e.handle(Command::Tick(at(15) - ms(100)));
        e.take();
        e.handle(Command::KeyFailed("no CAT".into()));
        let n = e.take();
        assert_eq!(n[0], Notice::Ptt(false));
        assert_eq!(n[1], Notice::Error("no CAT".into()));
        assert!(!e.setup().seq.enabled);
    }

    #[test]
    fn without_a_keyer_the_burst_comes_with_ptt() {
        let mut e = engine();
        enable(&mut e, |s| s.parity = Parity::Odd);
        e.take();
        e.handle(Command::Tick(at(15) - ms(100)));
        let n = e.take();
        assert_eq!(n[0], Notice::Ptt(true));
        assert!(matches!(&n[1], Notice::Burst(b) if b.at == at(15) + ms(500)));
    }

    #[test]
    fn the_dial_follows_the_mode_and_marks_spots() {
        let mut e = engine();
        e.handle(Command::Dial(14_074_000));
        let mut s = e.setup().clone();
        s.proto = Proto::Ft4;
        e.handle(Command::Setup(Box::new(s.clone())));
        assert_eq!(e.take()[0], Notice::Tune(14_080_000));
        e.handle(Command::Dial(14_080_000));
        e.handle(Command::Setup(Box::new(s.clone())));
        assert!(!e.take().iter().any(|n| matches!(n, Notice::Tune(_))));
        e.handle(Command::Dial(14_090_000));
        s.proto = Proto::Ft8;
        e.handle(Command::Setup(Box::new(s)));
        assert!(!e.take().iter().any(|n| matches!(n, Notice::Tune(_))));
        let mut d = decode("JA1ZLO JA1YWX R-07", -12.0);
        d.slot = Some(at(15));
        e.handle(Command::Select(Box::new(d)));
        let n = e.take();
        assert!(n
            .iter()
            .any(|n| matches!(n, Notice::QsoCompleted(q) if q.dial_hz == Some(14_090_000))));
    }

    #[test]
    fn nothing_is_decoded_while_stopped() {
        let mut e = Engine::new();
        let samples = |at| Command::Samples {
            rate: 12000,
            samples: vec![0.0; 12000],
            at,
        };
        e.handle(samples(at(0)));
        assert!(e.rx.is_none() && e.take().is_empty());
        e.handle(Command::Start);
        e.handle(samples(at(0)));
        assert!(e.rx.is_some());
        e.handle(Command::Stop);
        assert_eq!(e.take(), [Notice::Running(true), Notice::Running(false)]);
    }

    #[test]
    fn decodes_bring_spots_and_skew_and_latency_warnings() {
        use crate::sim::{render, Band, Source, Station};
        let mut e = engine();
        e.latency = Some(Duration::ZERO);
        let band = Band {
            clock_offset: 1.0,
            ..Band::slot(Source::Ft8)
        };
        let st = [Station::new("CQ JA1YWX PM95", 1200.0, 0.0, -5.0)];
        let x = render(&band, Source::Ft8, &st, 3).unwrap();
        let rate = band.rate;
        e.handle(Command::Samples {
            rate,
            samples: x,
            at: at(0),
        });
        e.handle(Command::Samples {
            rate,
            samples: vec![0.0; 6000],
            at: at(15),
        });
        let n = e.take();
        let spot = n.iter().find_map(|n| match n {
            Notice::Spot(s) => Some(s.clone()),
            _ => None,
        });
        let spot = spot.expect("a spot");
        assert_eq!(spot.dial_hz, None);
        assert_eq!(
            (spot.callsign.as_str(), spot.grid.as_deref()),
            ("JA1YWX", Some("PM95"))
        );
        let Some(Notice::Warnings(w)) = n.iter().find(|n| matches!(n, Notice::Warnings(_))) else {
            panic!("{n:?}")
        };
        assert!(
            matches!(w[0], Warning::ClockSkew(dt) if (dt - 1.0).abs() < 0.15),
            "{w:?}"
        );
        assert!(matches!(w[1], Warning::Latency(_)), "{w:?}");
    }
}
