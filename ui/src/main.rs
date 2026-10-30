#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::SystemTime;

use ftxl_ui::app::{self, Prefs};
use ftxl_ui::audio::{input_devices, output_devices, Input, Output};
use ftxl_ui::text::texts;
use ftxl_ui::{Event, FtPanel, FtTools};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 560.0])
            .with_min_inner_size([280.0, 320.0])
            .with_title("FTxL"),
        ..Default::default()
    };
    eframe::run_native(
        "FTxL",
        options,
        Box::new(|cc| {
            install_japanese_font(&cc.egui_ctx);
            Ok(Box::new(App::new(cc)))
        }),
    )
}

struct App {
    prefs: Prefs,
    ft: FtPanel,
    vox: bool,
    input_io: Option<Input>,
    output_io: Option<Output>,
    inputs: Vec<String>,
    outputs: Vec<String>,
    settings_open: bool,
    error: String,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k));
        let prefs = Prefs::load(get, system_language);
        let language = prefs.language.clone();
        let mut ft = FtPanel::new(&language);
        for (k, _) in ft.settings() {
            if let Some(v) = get(&format!("ft.{k}")) {
                ft.load_setting(k, &v);
            }
        }
        cc.egui_ctx.set_theme(prefs.theme);
        App {
            prefs,
            ft,
            vox: get("app.transmit").as_deref() == Some("vox"),
            input_io: None,
            output_io: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            settings_open: false,
            error: String::new(),
        }
    }

    fn run_audio(&mut self) {
        let tx = FtTools {
            enable: self.vox,
            halt: self.vox,
            ..FtTools::ALL
        };
        self.ft.tools = tx;
        let wanted = !self.ft.playing_wav() && self.ft.running();
        if wanted && self.input_io.is_none() {
            match Input::open(self.prefs.input.as_deref()) {
                Ok(i) => self.input_io = Some(i),
                Err(e) => {
                    self.error = e;
                    self.ft.stop();
                }
            }
        }
        if !wanted {
            self.input_io = None;
        }
        if self.vox && self.output_io.is_none() {
            match Output::open(self.prefs.output.as_deref()) {
                Ok(o) => {
                    self.ft.set_output_rate(o.rate);
                    self.output_io = Some(o);
                }
                Err(e) => {
                    self.error = e;
                    self.vox = false;
                }
            }
        }
        if !self.vox {
            self.output_io = None;
        }
        if let Some((x, at)) = self.input_io.as_mut().and_then(|i| i.take()) {
            let rate = self.input_io.as_ref().map_or(48000, |i| i.rate);
            self.ft.push_samples(rate, &x, at);
        }
    }

    fn handle(&mut self, events: Vec<Event>) {
        for e in events {
            match e {
                Event::Burst(b) => {
                    if let Some(o) = &self.output_io {
                        o.play(&b.lead_in(SystemTime::now()));
                    }
                }
                Event::Halt => {
                    if let Some(o) = &self.output_io {
                        o.stop();
                    }
                }
                Event::OpenWav => self.open_wav(),
                Event::Settings => self.settings_open = !self.settings_open,
                _ => {}
            }
        }
    }

    fn open_wav(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("WAV", &["wav", "WAV"])
            .pick_file()
        else {
            return;
        };
        if let Err(e) = self.ft.load_wav(&path) {
            self.error = e;
        }
    }

    fn settings_body(&mut self, ui: &mut egui::Ui) {
        if self.inputs.is_empty() {
            self.inputs = app::sound_cards(input_devices());
            self.outputs = app::sound_cards(output_devices());
        }
        let lang = self.prefs.language.clone();
        let t = |k| texts().get(&lang, k);
        let (input, output) = (self.prefs.input.clone(), self.prefs.output.clone());
        app::device_field(ui, t("input"), &lang, &mut self.prefs.input, &self.inputs);
        app::device_field(
            ui,
            t("output"),
            &lang,
            &mut self.prefs.output,
            &self.outputs,
        );
        if self.prefs.input != input {
            self.input_io = None;
        }
        if self.prefs.output != output {
            self.output_io = None;
        }
        ftxl_ui::field(ui, t("transmit"), |ui| {
            let label = t(if self.vox {
                "transmit.vox"
            } else {
                "transmit.off"
            });
            egui::ComboBox::from_id_salt("app-transmit")
                .selected_text(label)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.vox, false, t("transmit.off"));
                    ui.selectable_value(&mut self.vox, true, t("transmit.vox"));
                })
                .response
        });
        app::language_field(ui, &mut self.prefs.language);
        app::theme_field(ui, &lang, &mut self.prefs.theme);
        self.ft.language = self.prefs.language.clone();
        egui::CollapsingHeader::new("FT8 / FT4").show(ui, |ui| self.ft.settings_ui(ui));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.run_audio();
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::side_top_panel(&ctx.global_style())
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                self.ft.toolbar(ui);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.global_style()).inner_margin(8))
            .show(ui, |ui| {
                if !self.error.is_empty() {
                    let e = self.error.clone();
                    if ui
                        .colored_label(egui::Color32::from_rgb(220, 70, 70), e)
                        .clicked()
                    {
                        self.error.clear();
                    }
                }
                self.ft.body(ui);
            });
        let events = self.ft.take_events();
        self.handle(events);
        if self.settings_open {
            let mut open = true;
            let lang = self.prefs.language.clone();
            app::settings_window(&ctx, &lang, &mut open, |ui| self.settings_body(ui));
            self.settings_open = open;
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        for (k, v) in self.ft.settings() {
            storage.set_string(&format!("ft.{k}"), v);
        }
        self.prefs.save(|k, v| storage.set_string(k, v));
        let tx = if self.vox { "vox" } else { "off" };
        storage.set_string("app.transmit", tx.into());
    }
}

fn system_language() -> String {
    let lang = std::env::var("LANG").unwrap_or_default();
    if lang.starts_with("ja") || cfg!(target_os = "windows") && is_japanese_windows() {
        "ja".into()
    } else {
        "en".into()
    }
}

fn is_japanese_windows() -> bool {
    std::path::Path::new("C:\\Windows\\Fonts\\YuGothR.ttc").exists()
}

fn install_japanese_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "C:\\Windows\\Fonts\\YuGothR.ttc",
        "C:\\Windows\\Fonts\\meiryo.ttc",
        "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    ];
    let Some(bytes) = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "japanese".into(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("japanese".into());
    }
    ctx.set_fonts(fonts);
}
