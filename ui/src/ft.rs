use std::time::{Duration, SystemTime, UNIX_EPOCH};

use egui::{RichText, Ui};
use ftxl::{
    next_slot, slot_start, Command, Decode, Engine, Notice, Options, Parity, Proto, Sequencer,
    Setup, State, Warning, MESSAGES,
};

use crate::event::{Event, Events};
use crate::icon;
use crate::text::texts;
use crate::wav::Playback;

pub const MAX_DECODES: usize = 500;

const NARROW: f32 = 360.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FtTools {
    pub start: bool,
    pub mode: bool,
    pub parity: bool,
    pub enable: bool,
    pub halt: bool,
    pub wav: bool,
    pub settings: bool,
}

impl FtTools {
    pub const ALL: FtTools = FtTools {
        start: true,
        mode: true,
        parity: true,
        enable: true,
        halt: true,
        wav: true,
        settings: true,
    };

    pub const NONE: FtTools = FtTools {
        start: false,
        mode: false,
        parity: false,
        enable: false,
        halt: false,
        wav: false,
        settings: false,
    };
}

impl Default for FtTools {
    fn default() -> Self {
        FtTools::ALL
    }
}

pub struct FtPanel {
    pub language: String,
    pub tools: FtTools,
    pub proto: Proto,
    pub my_call: String,
    pub host_call: bool,
    pub my_grid: String,
    pub rx_hz: f32,
    pub tx_hz: f32,
    pub parity: Parity,
    pub options: Options,
    pub seq: Sequencer,
    engine: Option<Engine>,
    outbox: Vec<Command>,
    told: Setup,
    wav: Option<Playback>,
    sim_slot: Option<SystemTime>,
    decodes: Vec<Decode>,
    warnings: Vec<Warning>,
    running: bool,
    ptt: bool,
    events: Events,
    status: String,
}

impl Default for FtPanel {
    fn default() -> Self {
        FtPanel::new("en")
    }
}

impl FtPanel {
    pub fn new(language: &str) -> Self {
        let s = Setup::default();
        FtPanel {
            language: language.into(),
            tools: FtTools::ALL,
            proto: s.proto,
            my_call: String::new(),
            host_call: false,
            my_grid: String::new(),
            rx_hz: s.rx_hz,
            tx_hz: s.tx_hz,
            parity: s.parity,
            options: s.options,
            seq: s.seq.clone(),
            engine: Some(Engine::new()),
            outbox: Vec::new(),
            told: s,
            wav: None,
            sim_slot: None,
            decodes: Vec::new(),
            warnings: Vec::new(),
            running: false,
            ptt: false,
            events: Events::default(),
            status: String::new(),
        }
    }

    pub fn remote(language: &str) -> Self {
        FtPanel {
            engine: None,
            ..FtPanel::new(language)
        }
    }

    pub fn take_commands(&mut self) -> Vec<Command> {
        self.sync();
        std::mem::take(&mut self.outbox)
    }

    pub fn notify(&mut self, n: Notice) {
        match n {
            Notice::Running(on) => {
                self.running = on;
                self.events.emit(Event::Running(on));
            }
            Notice::Setup(s) => self.adopt(*s),
            Notice::Decode(d) => {
                self.decodes.push(d.clone());
                if self.decodes.len() > MAX_DECODES {
                    self.decodes.drain(..self.decodes.len() - MAX_DECODES);
                }
                self.events.emit(Event::Decode(d));
            }
            Notice::Spot(s) => self.events.emit(Event::Spot(s)),
            Notice::Pick(p) => self.events.emit(Event::Pick(p)),
            Notice::QsoCompleted(q) => self.events.emit(Event::QsoCompleted(q)),
            Notice::Burst(b) => {
                self.status.clear();
                self.events.emit(Event::Burst(b));
            }
            Notice::Halt => self.events.emit(Event::Halt),
            Notice::Ptt(on) => {
                self.ptt = on;
                self.events.emit(Event::Ptt(on));
            }
            Notice::Tune(hz) => self.events.emit(Event::Tune(hz)),
            Notice::Warnings(w) => {
                self.warnings = w.clone();
                self.events.emit(Event::Warnings(w));
            }
            Notice::Error(e) => self.status = e,
        }
    }

    fn setup(&self) -> Setup {
        let mut seq = self.seq.clone();
        seq.my_call = self.my_call.trim().to_ascii_uppercase();
        seq.my_grid = self.my_grid.trim().to_ascii_uppercase();
        Setup {
            proto: self.proto,
            rx_hz: self.rx_hz,
            tx_hz: self.tx_hz,
            parity: self.parity,
            options: self.options,
            seq,
        }
    }

    fn adopt(&mut self, s: Setup) {
        self.proto = s.proto;
        self.rx_hz = s.rx_hz;
        self.tx_hz = s.tx_hz;
        self.parity = s.parity;
        self.options = s.options;
        self.seq = s.seq.clone();
        self.told = s;
    }

    fn sync(&mut self) {
        let s = self.setup();
        if s != self.told {
            self.seq = s.seq.clone();
            self.told = s.clone();
            self.dispatch(Command::Setup(Box::new(s)));
        }
    }

    fn dispatch(&mut self, c: Command) {
        let Some(engine) = self.engine.as_mut() else {
            self.outbox.push(c);
            return;
        };
        engine.handle(c);
        for n in engine.take() {
            self.notify(n);
        }
    }

    fn send(&mut self, c: Command) {
        self.sync();
        self.dispatch(c);
    }

    fn t(&self, key: &'static str) -> &'static str {
        texts().get(&self.language, key)
    }

    pub fn settings(&self) -> Vec<(&'static str, String)> {
        let o = &self.options;
        let odd = self.parity == Parity::Odd;
        vec![
            ("mode", self.proto.id().into()),
            ("my_call", self.my_call.clone()),
            ("my_grid", self.my_grid.clone()),
            ("rx_hz", self.rx_hz.to_string()),
            ("tx_hz", self.tx_hz.to_string()),
            ("parity", if odd { "odd" } else { "even" }.into()),
            ("auto", self.seq.auto.to_string()),
            ("cq", self.seq.cq.clone()),
            ("exchange", self.seq.exchange.clone()),
            ("min_hz", o.min_hz.to_string()),
            ("max_hz", o.max_hz.to_string()),
            ("passes", o.ft.passes.to_string()),
            ("candidates", o.ft.candidates.to_string()),
            ("rx_width", o.ft.rx_width.to_string()),
            ("blanker", o.ft.blanker.to_string()),
            ("fading", o.ft.fading.to_string()),
        ]
    }

    pub fn load_setting(&mut self, key: &str, value: &str) {
        fn set<T: std::str::FromStr>(v: &mut T, s: &str) {
            if let Ok(x) = s.trim().parse() {
                *v = x;
            }
        }
        let o = &mut self.options;
        match key {
            "mode" => {
                self.proto = if value == "ft4" {
                    Proto::Ft4
                } else {
                    Proto::Ft8
                }
            }
            "my_call" => self.my_call = value.trim().to_ascii_uppercase(),
            "my_grid" => self.my_grid = value.trim().to_ascii_uppercase(),
            "rx_hz" => set(&mut self.rx_hz, value),
            "tx_hz" => set(&mut self.tx_hz, value),
            "parity" => {
                self.parity = if value == "odd" {
                    Parity::Odd
                } else {
                    Parity::Even
                }
            }
            "auto" => set(&mut self.seq.auto, value),
            "cq" if !value.trim().is_empty() => self.seq.cq = value.trim().to_ascii_uppercase(),
            "exchange" if !value.trim().is_empty() => self.seq.exchange = value.trim().into(),
            "min_hz" => set(&mut o.min_hz, value),
            "max_hz" => set(&mut o.max_hz, value),
            "passes" => set(&mut o.ft.passes, value),
            "candidates" => set(&mut o.ft.candidates, value),
            "rx_width" => set(&mut o.ft.rx_width, value),
            "blanker" => set(&mut o.ft.blanker, value),
            "fading" => set(&mut o.ft.fading, value),
            _ => {}
        }
        self.options = self.options.sanitized();
        self.rx_hz = self.rx_hz.clamp(0.0, self.options.max_hz);
        self.tx_hz = self.tx_hz.clamp(0.0, self.options.max_hz);
    }

    pub fn on_event(&mut self, handler: impl FnMut(&Event) + 'static) {
        self.events.on(handler);
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.events.take()
    }

    pub fn decodes(&self) -> &[Decode] {
        &self.decodes
    }

    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    pub fn state(&self) -> State {
        self.seq.state()
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn sending(&self) -> bool {
        self.ptt
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn set_output_rate(&mut self, rate: u32) {
        self.send(Command::OutputRate(rate));
    }

    pub fn start(&mut self) {
        if !self.running {
            self.status.clear();
        }
        self.send(Command::Start);
    }

    pub fn stop(&mut self) {
        self.wav = None;
        self.send(Command::Stop);
    }

    pub fn set_ptt_lead(&mut self, lead: Option<Duration>) {
        self.send(Command::PttLead(lead));
    }

    pub fn keyed(&mut self, on: bool) {
        self.send(Command::Keyed(on));
    }

    pub fn key_failed(&mut self, reason: &str) {
        self.send(Command::KeyFailed(reason.into()));
    }

    pub fn set_dial(&mut self, hz: u64) {
        self.send(Command::Dial(hz));
    }

    pub fn halt(&mut self) {
        self.send(Command::Halt);
    }

    pub fn push_samples(&mut self, rate: u32, samples: &[f32], at: SystemTime) {
        if self.wav.is_none() {
            self.feed(rate, samples, at);
        }
    }

    fn feed(&mut self, rate: u32, samples: &[f32], at: SystemTime) {
        if self.running || self.engine.is_none() {
            let samples = samples.to_vec();
            self.send(Command::Samples { rate, samples, at });
        }
    }

    pub fn play_wav(&mut self, samples: Vec<f32>, rate: u32) {
        self.start();
        self.wav = Some(Playback::new(samples, rate));
    }

    pub fn load_wav(&mut self, path: &std::path::Path) -> Result<(), String> {
        let (samples, rate) = crate::wav::read(path)?;
        self.play_wav(samples, rate);
        Ok(())
    }

    pub fn stop_wav(&mut self) {
        self.wav = None;
    }

    pub fn playing_wav(&self) -> bool {
        self.wav.is_some()
    }

    fn pump(&mut self, now: SystemTime) {
        let period = self.proto.period();
        let Some(w) = self.wav.as_mut() else {
            return;
        };
        let chunk = w.take(now, |t| slot_start(t, period));
        let done = w.done();
        if let Some((rate, x, at)) = chunk {
            self.feed(rate, &x, at);
        }
        if done {
            self.stop_wav();
        }
    }

    pub fn select(&mut self, n: usize) {
        if let Some(d) = self.decodes.get(n).cloned() {
            self.send(Command::Select(Box::new(d)));
        }
    }

    pub fn tick(&mut self, now: SystemTime) {
        self.pump(now);
        self.send(Command::Tick(now));
    }

    pub fn simulate(&mut self, text: &str, now: SystemTime) {
        let text = text.trim().to_ascii_uppercase();
        if text.is_empty() {
            return;
        }
        self.start();
        let period = self.proto.period();
        let source = match self.proto {
            Proto::Ft8 => ftxl::sim::Source::Ft8,
            Proto::Ft4 => ftxl::sim::Source::Ft4,
        };
        let mut band = ftxl::sim::Band::slot(source);
        band.rate = 12000;
        let station = ftxl::sim::Station::new(&text, self.tx_hz, 0.0, -5.0);
        let slot = next_slot(now, period, Parity::of(now, period));
        let slot = self.sim_slot.filter(|s| *s >= slot).unwrap_or(slot);
        match ftxl::sim::render(&band, source, &[station], 1) {
            Ok(mut audio) => {
                audio.resize((period.as_secs_f32() * band.rate as f32) as usize, 0.0);
                self.sim_slot = Some(slot + period);
                self.feed(band.rate, &audio, slot);
                self.feed(band.rate, &[0.0; 1200], slot + period);
            }
            Err(e) => self.status = e,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.toolbar(ui);
        ui.add_space(4.0);
        self.body(ui);
    }

    pub fn toolbar(&mut self, ui: &mut Ui) {
        self.toolbar_ui(ui, |_| {});
    }

    pub fn toolbar_ui(&mut self, ui: &mut Ui, mode: impl FnOnce(&mut Ui)) {
        let settings = self.tools.settings;
        let label = self.t("settings");
        let mut open = false;
        crate::bar(
            ui,
            |ui| open = settings && icon::button(ui, icon::GEAR, label, false).clicked(),
            |ui| self.tools_ui(ui, mode),
        );
        if open {
            self.events.emit(Event::Settings);
        }
    }

    pub fn tools_ui(&mut self, ui: &mut Ui, mode: impl FnOnce(&mut Ui)) {
        let tools = self.tools;
        ui.spacing_mut().item_spacing.x = 6.0;
        if tools.start {
            let (g, label) = if self.running {
                (icon::STOP, self.t("stop"))
            } else {
                (icon::PLAY, self.t("start"))
            };
            if icon::button(ui, g, label, self.running).clicked() {
                if self.running {
                    self.stop();
                } else {
                    self.start();
                }
            }
        }
        if tools.wav {
            if self.wav.is_some() {
                if icon::button(ui, icon::STOP, self.t("stop_wav"), false).clicked() {
                    self.stop_wav();
                }
            } else if icon::button(ui, icon::FOLDER_OPEN, self.t("open_wav"), false).clicked() {
                self.events.emit(Event::OpenWav);
            }
        }
        mode(ui);
        if tools.mode {
            for p in Proto::ALL {
                ui.selectable_value(&mut self.proto, p, p.name())
                    .on_hover_text(self.t(if p == Proto::Ft8 {
                        "ft8.tip"
                    } else {
                        "ft4.tip"
                    }));
            }
        }
        if tools.parity {
            let (even, odd) = (self.t("even"), self.t("odd"));
            let (even_tip, odd_tip) = (self.t("even.tip"), self.t("odd.tip"));
            ui.selectable_value(&mut self.parity, Parity::Even, even)
                .on_hover_text(even_tip);
            ui.selectable_value(&mut self.parity, Parity::Odd, odd)
                .on_hover_text(odd_tip);
        }
        if tools.enable {
            let (label, tip) = (self.t("enable"), self.t("enable.tip"));
            ui.toggle_value(&mut self.seq.enabled, label)
                .on_hover_text(tip);
        }
        if tools.halt {
            let tip = self.t("halt.tip");
            let mut b = egui::Button::new(self.t("halt"));
            if self.ptt {
                b = b.fill(crate::alert(ui.visuals()).gamma_multiply(0.5));
            }
            let r = ui.add_enabled(self.ptt, b).on_hover_text(tip);
            if r.clicked() {
                self.halt();
            }
        }
        self.sync();
    }

    pub fn body(&mut self, ui: &mut Ui) {
        let now = SystemTime::now();
        self.tick(now);
        if self.running || self.wav.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if !self.status.is_empty() {
            ui.colored_label(crate::alert(ui.visuals()), &self.status);
        }
        self.clock_ui(ui, now);
        self.decodes_ui(ui);
        ui.separator();
        self.band_ui(ui);
        self.dx_ui(ui);
        self.messages_ui(ui);
        self.sync();
    }

    fn clock_ui(&mut self, ui: &mut Ui, now: SystemTime) {
        let period = self.proto.period();
        let start = slot_start(now, period);
        let secs = start
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() % 86400);
        let into = now.duration_since(start).unwrap_or_default();
        let parity = match Parity::of(start, period) {
            Parity::Even => self.t("even"),
            Parity::Odd => self.t("odd"),
        };
        let utc = format!(
            "{:02}:{:02}:{:02} {parity}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        );
        let alert = crate::alert(ui.visuals());
        ui.horizontal(|ui| {
            ui.label(RichText::new(utc).monospace())
                .on_hover_text(self.t("utc.tip"));
            for w in &self.warnings {
                let (text, tip) = match w {
                    Warning::ClockSkew(dt) => (
                        texts().format(&self.language, "skew", &[&format!("{dt:+.1}")]),
                        self.t("skew.tip"),
                    ),
                    Warning::Latency(d) => (
                        texts().format(
                            &self.language,
                            "latency",
                            &[&format!("{:.1}", d.as_secs_f32())],
                        ),
                        self.t("latency.tip"),
                    ),
                };
                ui.colored_label(alert, text).on_hover_text(tip);
            }
            let fraction = into.as_secs_f32() / period.as_secs_f32();
            ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).desired_height(6.0));
        });
    }

    fn line(&self, d: &Decode, narrow: bool) -> String {
        let secs = d
            .slot
            .and_then(|s| s.duration_since(UNIX_EPOCH).ok())
            .map(|s| s.as_secs() % 86400);
        let utc = secs.map_or("------".into(), |s| {
            format!("{:02}{:02}{:02}", s / 3600, s / 60 % 60, s % 60)
        });
        if narrow {
            format!("{:>3.0} {:>4.0} {}", d.snr, d.hz, d.text)
        } else {
            let dt = d.dt.unwrap_or(0.0);
            format!("{utc} {:>3.0} {dt:>4.1} {:>4.0} {}", d.snr, d.hz, d.text)
        }
    }

    fn decodes_ui(&mut self, ui: &mut Ui) {
        let narrow = ui.available_width() < NARROW;
        let row = ui.spacing().interact_size.y;
        let reserved = (MESSAGES as f32 + 5.0) * (row + ui.spacing().item_spacing.y);
        let height = (ui.available_height() - reserved).max(4.0 * row);
        let header = self.t(if narrow { "columns.narrow" } else { "columns" });
        ui.label(RichText::new(header).monospace().weak());
        let my_call = self.my_call.trim().to_ascii_uppercase();
        let (cq, mine) = (crate::accent(ui.visuals()), crate::alert(ui.visuals()));
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("ft-decodes")
            .max_height(height)
            .auto_shrink([false, true])
            .stick_to_bottom(true)
            .show_rows(ui, row, self.decodes.len(), |ui, range| {
                for n in range {
                    let d = &self.decodes[n];
                    let to = d.fields.as_ref().map_or("", |f| f.to.as_str());
                    let mut text = RichText::new(self.line(d, narrow)).monospace();
                    if !my_call.is_empty() && to == my_call {
                        text = text.color(mine).strong();
                    } else if to.starts_with("CQ") {
                        text = text.color(cq);
                    }
                    let b = egui::Button::selectable(false, text)
                        .truncate()
                        .min_size(egui::vec2(ui.available_width(), row));
                    if ui.add(b).clicked() {
                        picked = Some(n);
                    }
                }
            });
        if self.decodes.is_empty() {
            let key = match (self.wav.is_some(), self.running) {
                (true, _) => "decoding",
                (_, true) => "waiting",
                _ => "idle",
            };
            ui.weak(self.t(key));
        }
        if let Some(n) = picked {
            self.select(n);
        }
    }

    fn band_ui(&mut self, ui: &mut Ui) {
        let top = self.options.max_hz.max(1.0);
        let width = ui.available_width();
        let (rect, r) = ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::click());
        let x = |hz: f32| rect.left() + rect.width() * (hz / top).clamp(0.0, 1.0);
        let p = ui.painter();
        p.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
        let last = self.decodes.last().and_then(|d| d.slot);
        let weak = ui.visuals().weak_text_color();
        for d in self.decodes.iter().rev().take_while(|d| d.slot == last) {
            p.vline(x(d.hz), rect.y_range(), egui::Stroke::new(1.0, weak));
        }
        let mark = |hz: f32, c: egui::Color32| {
            let r = egui::Rect::from_x_y_ranges(x(hz)..=x(hz + 50.0), rect.y_range());
            p.rect_filled(r, 0.0, c.gamma_multiply(0.6));
        };
        mark(self.rx_hz, crate::good(ui.visuals()));
        mark(self.tx_hz, crate::alert(ui.visuals()));
        let hz = |pos: egui::Pos2| ((pos.x - rect.left()) / rect.width() * top).round();
        if let Some(pos) = r.interact_pointer_pos() {
            if r.clicked() {
                self.tx_hz = hz(pos);
            } else if r.secondary_clicked() {
                self.rx_hz = hz(pos);
            }
        }
        let tip = self.t("band.tip");
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, tip));
        r.on_hover_text(tip);
        ui.horizontal_wrapped(|ui| {
            let (rx, tx, same) = (self.t("rx"), self.t("tx"), self.t("tx_to_rx"));
            let range = 0.0..=top;
            ui.label(rx);
            ui.add(
                egui::DragValue::new(&mut self.rx_hz)
                    .range(range.clone())
                    .suffix(" Hz"),
            );
            ui.label(tx);
            ui.add(
                egui::DragValue::new(&mut self.tx_hz)
                    .range(range)
                    .suffix(" Hz"),
            );
            if ui
                .button(same)
                .on_hover_text(self.t("tx_to_rx.tip"))
                .clicked()
            {
                self.tx_hz = self.rx_hz;
            }
        });
    }

    fn dx_ui(&mut self, ui: &mut Ui) {
        let (dx, hint) = (self.t("dx"), self.t("dx.call"));
        let state = match self.seq.state() {
            State::Idle => self.t("state.idle"),
            State::Reply => self.t("state.reply"),
            State::Exchange => self.t("state.exchange"),
            State::Confirm => self.t("state.confirm"),
            State::Log => self.t("state.log"),
        };
        let tip = self.t("state.tip");
        ui.horizontal(|ui| {
            ui.label(dx);
            let w = ui.available_width() - 80.0;
            ui.add(
                egui::TextEdit::singleline(&mut self.seq.his_call)
                    .hint_text(hint)
                    .desired_width(w.max(60.0)),
            );
            ui.weak(state).on_hover_text(tip);
        });
        self.seq.his_call.make_ascii_uppercase();
    }

    fn messages_ui(&mut self, ui: &mut Ui) {
        let s = self.setup();
        egui::Grid::new("ft-messages")
            .num_columns(2)
            .spacing([6.0, 2.0])
            .show(ui, |ui| {
                for n in 1..=MESSAGES {
                    let text = s.seq.message(n);
                    ui.radio_value(&mut self.seq.tx, n, format!("Tx{n}"));
                    ui.add(egui::Label::new(RichText::new(text).monospace()).truncate());
                    ui.end_row();
                }
            });
    }

    pub fn settings_ui(&mut self, ui: &mut Ui) {
        let t = |k| texts().get(&self.language, k);
        if !self.host_call {
            crate::field(ui, t("my_call"), |ui| {
                ui.text_edit_singleline(&mut self.my_call)
            });
        }
        crate::field(ui, t("my_grid"), |ui| {
            ui.text_edit_singleline(&mut self.my_grid)
        });
        crate::field(ui, t("cq"), |ui| {
            ui.text_edit_singleline(&mut self.seq.cq)
                .on_hover_text(t("cq.tip"))
        });
        crate::field(ui, t("exchange"), |ui| {
            ui.text_edit_singleline(&mut self.seq.exchange)
                .on_hover_text(t("exchange.tip"))
        });
        let o = &mut self.options;
        fn hz(v: &mut f32, lo: f32, hi: f32) -> egui::DragValue<'_> {
            egui::DragValue::new(v).range(lo..=hi).suffix(" Hz")
        }
        crate::field(ui, t("dx.grid"), |ui| {
            ui.add(egui::TextEdit::singleline(&mut self.seq.his_grid).char_limit(6))
        });
        crate::field(ui, t("report.tip"), |ui| {
            ui.add(egui::DragValue::new(&mut self.seq.report).range(-30..=30))
        });
        crate::field(ui, t("min_hz"), |ui| ui.add(hz(&mut o.min_hz, 0.0, 5000.0)));
        crate::field(ui, t("max_hz"), |ui| {
            ui.add(hz(&mut o.max_hz, 100.0, ftxl::MAX_HZ))
        });
        crate::field(ui, t("rx_width"), |ui| {
            ui.add(hz(&mut o.ft.rx_width, 0.0, 1000.0))
        });
        crate::field(ui, t("passes"), |ui| {
            ui.add(egui::DragValue::new(&mut o.ft.passes).range(1..=10))
        });
        crate::field(ui, t("candidates"), |ui| {
            ui.add(egui::DragValue::new(&mut o.ft.candidates).range(1..=5000))
        });
        crate::flags(
            ui,
            &mut [
                (&mut self.seq.auto, t("auto")),
                (&mut o.ft.blanker, t("blanker")),
                (&mut o.ft.fading, t("fading")),
            ],
        );
        self.my_call = self.my_call.trim().to_ascii_uppercase();
        self.my_grid = self.my_grid.trim().to_ascii_uppercase();
        self.seq.his_grid = self.seq.his_grid.trim().to_ascii_uppercase();
        self.seq.cq = self.seq.cq.to_ascii_uppercase();
        self.options = self.options.sanitized();
        self.sync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_793_000_000 / 30 * 30 + secs)
    }

    #[test]
    fn settings_round_trip() {
        let mut a = FtPanel::new("en");
        a.proto = Proto::Ft4;
        a.my_grid = "PM95".into();
        a.rx_hz = 812.0;
        a.parity = Parity::Odd;
        a.seq.auto = false;
        a.seq.exchange = "$G".into();
        a.options.ft.passes = 2;
        a.options.ft.fading = false;
        a.load_setting("my_call", " ja1zlo ");
        a.load_setting("cq", "cq test");
        let mut b = FtPanel::new("ja");
        for (k, v) in a.settings() {
            b.load_setting(k, &v);
        }
        assert_eq!(b.settings(), a.settings());
        assert_eq!(
            (b.my_call.as_str(), b.seq.cq.as_str()),
            ("JA1ZLO", "CQ TEST")
        );
        b.load_setting("passes", "x");
        b.load_setting("rx_hz", "99999");
        assert_eq!((b.options.ft.passes, b.rx_hz), (2, 3000.0));
    }

    #[test]
    fn a_remote_panel_speaks_only_commands_and_notices() {
        let mut local = Engine::new();
        let mut p = FtPanel::remote("en");
        p.my_call = "JA1ZLO".into();
        p.start();
        p.push_samples(12000, &[0.0; 1200], at(0));
        let sent = p.take_commands();
        assert!(matches!(sent[0], Command::Setup(_)));
        assert!(matches!(sent[1], Command::Start));
        assert!(matches!(sent[2], Command::Samples { .. }));
        assert!(!p.running());
        for c in sent {
            local.handle(c);
        }
        for n in local.take() {
            p.notify(n);
        }
        assert!(p.running() && local.setup().seq.my_call == "JA1ZLO");
        assert_eq!(p.take_events(), [Event::Running(true)]);
        p.seq.enabled = true;
        p.tick(at(29) + Duration::from_millis(950));
        for c in p.take_commands() {
            local.handle(c);
        }
        for n in local.take() {
            p.notify(n);
        }
        assert!(p.sending());
        assert!(p.take_events().iter().any(|e| matches!(e, Event::Burst(_))));
    }

    #[test]
    fn engine_changes_come_back_to_the_panel_fields() {
        let mut p = FtPanel::new("en");
        p.my_call = "JA1YWX".into();
        p.simulate("CQ JA1ZLO PM95", at(0));
        p.select(0);
        assert!(p.seq.enabled && p.seq.tx == 1 && p.seq.his_call == "JA1ZLO");
        assert_eq!((p.state(), p.rx_hz), (State::Reply, 1500.0));
        assert_eq!(p.parity, Parity::Odd);
    }
}
