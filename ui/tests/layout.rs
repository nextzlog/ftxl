use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use std::time::{Duration, SystemTime};

use ftxl_ui::FtPanel;

const START: &str = "Start";
const STOP: &str = "Stop";
const OPEN_WAV: &str = "Open…";
const STOP_WAV: &str = "Stop WAV";
const SETTINGS: &str = "Settings";

fn builder<S>() -> egui_kittest::HarnessBuilder<S> {
    Harness::builder()
}

const WIDTHS: [f32; 3] = [280.0, 360.0, 600.0];

const CONTROLS: [Role; 6] = [
    Role::Button,
    Role::RadioButton,
    Role::TextInput,
    Role::SpinButton,
    Role::CheckBox,
    Role::Slider,
];

fn inside<S>(h: &Harness<'_, S>, width: f32) {
    let mut n = 0;
    for node in h.query_all_by(|n| CONTROLS.contains(&n.role())) {
        let r = node.rect();
        assert!(
            r.min.x >= -0.5 && r.max.x <= width + 0.5,
            "{:?} {:?} at {width}",
            node.accesskit_node().label(),
            r
        );
        n += 1;
    }
    assert!(n > 0);
    assert!(h.query_by_role(Role::ComboBox).is_none());
}

fn in_order<S>(h: &Harness<'_, S>, labels: &[&str]) {
    let rects: Vec<egui::Rect> = labels.iter().map(|l| h.get_by_label(l).rect()).collect();
    for (w, l) in rects.windows(2).zip(labels.windows(2)) {
        let (a, b) = (w[0], w[1]);
        let same_row = (a.center().y - b.center().y).abs() < 2.0;
        assert!(
            (same_row && a.max.x <= b.min.x) || a.max.y <= b.min.y + 1.0,
            "{l:?} {a:?} {b:?}"
        );
    }
}

fn right_top<S>(h: &Harness<'_, S>, width: f32) {
    let s = h.get_by_label(SETTINGS).rect();
    let start = h.get_by_label(START).rect();
    assert!(s.max.x > width - 12.0 && s.min.y < 20.0, "{s:?} at {width}");
    assert!(start.max.x <= s.min.x || start.max.y <= s.min.y + 1.0);
}

fn ft(width: f32, panel: FtPanel) -> Harness<'static, FtPanel> {
    let mut h = builder()
        .with_size([width, 560.0])
        .build_ui_state(|ui, p: &mut FtPanel| p.ui(ui), panel);
    h.run_steps(3);
    h
}

#[test]
fn the_ft_panel_fits_every_width_with_its_controls_in_order() {
    for w in WIDTHS {
        let h = ft(w, FtPanel::new("en"));
        inside(&h, w);
        in_order(
            &h,
            &[
                START,
                OPEN_WAV,
                "FT8",
                "FT4",
                "Even",
                "Odd",
                "Enable Tx",
                "Halt",
            ],
        );
        in_order(&h, &["Halt", "Tx1", "Tx6"]);
        right_top(&h, w);
        for gone in ["Grid", "DX grid", "Report to send", "Mark", "Light"] {
            assert!(h.query_by_label(gone).is_none(), "{gone} at {w}");
        }
        let start = h.get_by_label(START).rect();
        let halt = h.get_by_label("Halt").rect();
        assert!(start.min.y < 30.0, "the toolbar is the top row");
        if w >= 600.0 {
            assert!(
                (start.center().y - halt.center().y).abs() < 2.0,
                "one row at {w}"
            );
        }
    }
}

#[test]
fn the_ft_panel_works_in_a_tall_narrow_dock() {
    let mut p = FtPanel::new("en");
    p.simulate("CQ JA1ZLO PM95", SystemTime::now());
    p.stop();
    let mut h = builder()
        .with_size([280.0, 800.0])
        .build_ui_state(|ui, p: &mut FtPanel| p.ui(ui), p);
    h.run_steps(3);
    inside(&h, 280.0);
    right_top(&h, 280.0);
    in_order(&h, &[START, "Halt", "Tx1", "Tx6"]);
    let list = h.get_by_label_contains("CQ JA1ZLO PM95").rect();
    let tx6 = h.get_by_label("Tx6").rect();
    assert!(
        tx6.max.y <= 800.0 && list.width() > 240.0,
        "{list:?} {tx6:?}"
    );
}

#[test]
fn hidden_tools_are_not_drawn() {
    let mut p = FtPanel::new("en");
    p.tools.parity = false;
    p.tools.halt = false;
    let h = ft(360.0, p);
    assert!(h.query_by_label("Odd").is_none() && h.query_by_label("Halt").is_none());
    assert!(h.query_by_label("Enable Tx").is_some());
    let mut p = FtPanel::new("en");
    p.tools = ftxl_ui::FtTools::NONE;
    let h = ft(360.0, p);
    assert!(h.query_by_label("FT8").is_none() && h.query_by_label(START).is_none());
    assert!(h.query_by_label(SETTINGS).is_none());
}

#[test]
fn start_toggles_and_a_clicked_decode_enables_tx() {
    let mut h = ft(600.0, FtPanel::new("en"));
    h.get_by_label(START).click();
    h.run_steps(2);
    assert!(h.state().running());
    h.get_by_label(STOP).click();
    h.run_steps(2);
    assert!(!h.state().running());
    let mut p = FtPanel::new("en");
    p.my_call = "JA1YWX".into();
    p.simulate("CQ JA1ZLO PM95", std::time::SystemTime::now());
    let mut h = ft(600.0, p);
    h.get_by_label_contains("CQ JA1ZLO PM95").click();
    h.run_steps(3);
    let p = h.state();
    assert!(p.seq.enabled && p.seq.tx == 1 && p.seq.his_call == "JA1ZLO");
    assert_eq!(p.seq.next(), "JA1ZLO JA1YWX");
}

fn wav_file(name: &str, audio: &[f32], rate: u32) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ftxl-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    ftxl_ui::wav::write(&path, audio, rate).unwrap();
    path
}

#[test]
fn open_wav_asks_the_host_and_a_chosen_file_plays_into_the_decoder() {
    let mut h = ft(280.0, FtPanel::new("en"));
    assert!(h.query_by_label("Play").is_none());
    assert!(h
        .query_by_label_contains("Press Start or open a WAV file")
        .is_some());
    h.get_by_label(OPEN_WAV).click();
    h.run_steps(2);
    assert_eq!(h.state_mut().take_events(), [ftxl_ui::Event::OpenWav]);
    h.get_by_label(SETTINGS).click();
    h.run_steps(2);
    assert_eq!(h.state_mut().take_events(), [ftxl_ui::Event::Settings]);
    let source = ftxl::sim::Source::Ft8;
    let mut band = ftxl::sim::Band::slot(source);
    band.rate = 12000;
    let station = ftxl::sim::Station::new("CQ JA1ZLO PM95", 1500.0, 0.0, -5.0);
    let mut audio = ftxl::sim::render(&band, source, &[station], 1).unwrap();
    audio.resize(15 * 12000, 0.0);
    let path = wav_file("ft8.wav", &audio, 12000);
    let now = SystemTime::now();
    h.state_mut().load_wav(&path).unwrap();
    h.state_mut().tick(now);
    h.run_steps(2);
    assert!(h.state().running() && h.state().playing_wav());
    assert!(h.query_by_label(STOP_WAV).is_some());
    h.state_mut().tick(now + Duration::from_secs(40));
    h.run_steps(2);
    let texts: Vec<String> = h.state().decodes().iter().map(|d| d.text.clone()).collect();
    assert_eq!(texts, ["CQ JA1ZLO PM95"]);
    assert!(!h.state().playing_wav());
    assert!(h.query_by_label(OPEN_WAV).is_some());

    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn settings_fields_fill_the_dialog_width() {
    for w in [280.0, 600.0] {
        let mut h = builder()
            .with_size([w, 700.0])
            .build_ui_state(|ui, p: &mut FtPanel| p.settings_ui(ui), FtPanel::new("en"));
        h.run_steps(2);
        for f in h.get_all_by_role(Role::TextInput) {
            let r = f.rect();
            assert!(r.max.x <= w + 0.5 && r.max.x > w - 24.0, "{r:?} at {w}");
        }
    }
}

#[test]
fn the_decode_list_fills_the_width() {
    for w in WIDTHS {
        let now = SystemTime::now();
        let mut p = FtPanel::new("en");
        p.simulate("CQ JA1ZLO PM95", now);
        let h = ft(w, p);
        let panel = w - 16.0;
        let decode = h.get_by_label_contains("CQ JA1ZLO PM95").rect();
        let band = h.get_by_label("Click for Tx, right-click for Rx").rect();
        let full = 0.95 * panel;
        assert!(
            decode.width() >= full && band.width() >= full,
            "{decode:?} {band:?} at {w}"
        );
        inside(&h, w);
    }
}
