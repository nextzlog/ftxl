use crate::modem::Decode;

pub const MESSAGES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Reply,
    Exchange,
    Confirm,
    Log,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub call: String,
    pub grid: String,
    pub sent: String,
    pub rcvd: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequencer {
    pub my_call: String,
    pub my_grid: String,
    pub his_call: String,
    pub his_grid: String,
    pub report: i32,
    pub rcvd: Option<String>,
    pub cq: String,
    pub exchange: String,
    pub tx: usize,
    pub enabled: bool,
    pub auto: bool,
    logged: bool,
}

impl Default for Sequencer {
    fn default() -> Self {
        Sequencer {
            my_call: String::new(),
            my_grid: String::new(),
            his_call: String::new(),
            his_grid: String::new(),
            report: -10,
            rcvd: None,
            cq: "CQ".into(),
            exchange: "$R".into(),
            tx: 6,
            enabled: false,
            auto: true,
            logged: false,
        }
    }
}

enum Extra {
    Grid(String),
    Exchange(String),
    Roger(String),
    Rogered,
    Bye,
    Other,
}

fn is_grid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 4
        && (b'A'..=b'R').contains(&b[0])
        && (b'A'..=b'R').contains(&b[1])
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
        && s != "RR73"
}

fn report(s: &str) -> Option<i32> {
    s.starts_with(['+', '-']).then(|| s.parse().ok()).flatten()
}

fn exchange(s: &str) -> bool {
    is_grid(s) || report(s).is_some()
}

fn extra(s: &str) -> Extra {
    let roger = s
        .strip_prefix("R ")
        .or_else(|| s.strip_prefix('R').filter(|r| report(r).is_some()));
    match s {
        "RR73" | "RRR" => Extra::Rogered,
        "73" => Extra::Bye,
        s if is_grid(s) => Extra::Grid(s.into()),
        s if report(s).is_some() => Extra::Exchange(s.into()),
        _ => match roger.filter(|r| exchange(r)) {
            Some(r) => Extra::Roger(r.into()),
            None => Extra::Other,
        },
    }
}

pub fn format_report(db: i32) -> String {
    format!("{:+03}", db.clamp(-30, 30))
}

fn field<'a>(d: &'a Decode, key: &str) -> &'a str {
    d.fields.as_ref().map_or("", |f| match key {
        "to" => f.to.as_str(),
        "de" => f.de.as_str(),
        _ => f.extra.as_str(),
    })
}

impl Sequencer {
    pub fn state(&self) -> State {
        match (self.enabled, self.tx) {
            (false, _) | (_, 6) => State::Idle,
            (_, 1) => State::Reply,
            (_, 2 | 3) => State::Exchange,
            (_, 4) => State::Confirm,
            _ => State::Log,
        }
    }

    pub fn sent_exchange(&self) -> String {
        self.exchange
            .replace("$R", &format_report(self.report))
            .replace("$G", &self.my_grid)
            .trim()
            .to_string()
    }

    pub fn message(&self, n: usize) -> String {
        let (his, my) = (self.his_call.as_str(), self.my_call.as_str());
        let x = self.sent_exchange();
        let gap = if x.starts_with(['+', '-']) { "" } else { " " };
        match n {
            1 => format!("{his} {my} {}", self.my_grid),
            2 => format!("{his} {my} {x}"),
            3 => format!("{his} {my} R{gap}{x}"),
            4 => format!("{his} {my} RR73"),
            5 => format!("{his} {my} 73"),
            _ => format!("{} {my} {}", self.cq, self.my_grid),
        }
        .trim()
        .to_string()
    }

    pub fn next(&self) -> String {
        self.message(self.tx)
    }

    fn reset(&mut self, call: &str) {
        if call != self.his_call {
            self.his_call = call.into();
            self.his_grid.clear();
        }
        self.rcvd = None;
        self.logged = false;
    }

    pub fn select(&mut self, d: &Decode) -> Option<Contact> {
        let (to, de) = (field(d, "to"), field(d, "de"));
        if de.is_empty() || de == self.my_call {
            return None;
        }
        let mine = to == self.my_call;
        self.reset(de);
        self.report = d.snr.round() as i32;
        self.tx = 1;
        self.enabled = true;
        if mine {
            return self.heard(d);
        }
        if let Extra::Grid(g) = extra(field(d, "extra")) {
            self.his_grid = g;
        }
        None
    }

    pub fn heard(&mut self, d: &Decode) -> Option<Contact> {
        let (to, de) = (field(d, "to"), field(d, "de"));
        if !self.auto || to != self.my_call || de.is_empty() {
            return None;
        }
        let snr = d.snr.round() as i32;
        if de != self.his_call {
            if self.tx != 6 || !matches!(extra(field(d, "extra")), Extra::Grid(_)) {
                return None;
            }
            self.reset(de);
        }
        match extra(field(d, "extra")) {
            Extra::Grid(g) if self.tx == 6 => {
                self.his_grid = g;
                self.report = snr;
                self.tx = 2;
            }
            Extra::Grid(x) | Extra::Exchange(x) => {
                if is_grid(&x) {
                    self.his_grid = x.clone();
                }
                self.rcvd = Some(x);
                self.tx = 3;
            }
            Extra::Roger(x) => {
                if is_grid(&x) {
                    self.his_grid = x.clone();
                }
                self.rcvd = Some(x);
                self.tx = 4;
                return self.log();
            }
            Extra::Rogered => {
                self.tx = 5;
                return self.log();
            }
            Extra::Bye if self.tx == 4 => {
                let logged = self.log();
                self.enabled = false;
                return logged;
            }
            _ => return None,
        }
        self.enabled = true;
        None
    }

    pub fn sent(&mut self) {
        if self.tx == 5 || (self.tx == 4 && self.logged) {
            self.enabled = false;
        }
    }

    fn log(&mut self) -> Option<Contact> {
        self.enabled = true;
        if self.logged {
            return None;
        }
        self.logged = true;
        Some(Contact {
            call: self.his_call.clone(),
            grid: self.his_grid.clone(),
            sent: self.sent_exchange(),
            rcvd: self.rcvd.clone().unwrap_or_default(),
        })
    }

    pub fn halt(&mut self) {
        self.enabled = false;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn decode(text: &str, snr: f32) -> Decode {
        Decode {
            slot: None,
            at: std::time::SystemTime::UNIX_EPOCH,
            snr,
            hz: 900.0,
            dt: Some(0.0),
            text: text.into(),
            fields: Some(crate::fields(text).expect(text)),
        }
    }

    fn station(call: &str) -> Sequencer {
        Sequencer {
            my_call: call.into(),
            my_grid: "PM95".into(),
            ..Sequencer::default()
        }
    }

    #[test]
    fn the_standard_messages() {
        let mut s = station("JA1ZLO");
        s.his_call = "JA1YWX".into();
        s.report = -7;
        let all: Vec<String> = (1..=MESSAGES).map(|n| s.message(n)).collect();
        assert_eq!(
            all,
            [
                "JA1YWX JA1ZLO PM95",
                "JA1YWX JA1ZLO -07",
                "JA1YWX JA1ZLO R-07",
                "JA1YWX JA1ZLO RR73",
                "JA1YWX JA1ZLO 73",
                "CQ JA1ZLO PM95"
            ]
        );
        for m in &all {
            assert!(crate::pack(m).is_ok(), "{m}");
        }
        assert_eq!(format_report(5), "+05");
    }

    #[test]
    fn the_host_sets_the_exchange_and_the_cq() {
        let mut s = station("JA1ZLO");
        s.his_call = "JA1YWX".into();
        s.cq = "CQ TEST".into();
        s.exchange = "$G".into();
        assert_eq!(s.message(6), "CQ TEST JA1ZLO PM95");
        assert_eq!(s.message(2), "JA1YWX JA1ZLO PM95");
        assert_eq!(s.message(3), "JA1YWX JA1ZLO R PM95");
        for n in 1..=MESSAGES {
            assert!(crate::pack(&s.message(n)).is_ok(), "{}", s.message(n));
        }
        s.tx = 1;
        s.enabled = true;
        assert_eq!(s.heard(&decode("JA1ZLO JA1YWX PM85", 0.0)), None);
        assert_eq!((s.tx, s.state()), (3, State::Exchange));
        let c = s.heard(&decode("JA1ZLO JA1YWX RR73", 0.0)).unwrap();
        assert_eq!((c.sent.as_str(), c.rcvd.as_str()), ("PM95", "PM85"));
    }

    #[test]
    fn the_messages_follow_the_decoded_fields() {
        let mut a = station("JA1ZLO");
        let mut b = station("JA1YWX");
        a.enabled = true;
        assert_eq!(a.state(), State::Idle);
        assert_eq!(b.select(&decode("CQ JA1ZLO PM95", -3.0)), None);
        assert_eq!((b.tx, b.his_grid.as_str()), (1, "PM95"));
        assert_eq!(b.state(), State::Reply);
        assert_eq!(a.heard(&decode("JA1ZLO JA1YWX PM95", -12.0)), None);
        assert_eq!(a.next(), "JA1YWX JA1ZLO -12");
        assert_eq!(a.state(), State::Exchange);
        assert_eq!(b.heard(&decode("JA1YWX JA1ZLO -12", -4.0)), None);
        assert_eq!(b.next(), "JA1ZLO JA1YWX R-03");
        let logged = a.heard(&decode("JA1ZLO JA1YWX R-03", -11.0)).unwrap();
        assert_eq!((logged.sent.as_str(), logged.rcvd.as_str()), ("-12", "-03"));
        assert_eq!(a.next(), "JA1YWX JA1ZLO RR73");
        assert_eq!(a.state(), State::Confirm);
        let logged = b.heard(&decode("JA1YWX JA1ZLO RR73", -5.0)).unwrap();
        assert_eq!(
            (
                logged.call.as_str(),
                logged.sent.as_str(),
                logged.rcvd.as_str()
            ),
            ("JA1ZLO", "-03", "-12")
        );
        assert_eq!(b.next(), "JA1ZLO JA1YWX 73");
        assert_eq!(b.state(), State::Log);
        b.sent();
        assert!(!b.enabled);
        assert_eq!(a.heard(&decode("JA1ZLO JA1YWX 73", -5.0)), None);
        assert!(!a.enabled);
    }

    #[test]
    fn others_and_strangers_do_not_move_the_sequence() {
        let mut a = station("JA1ZLO");
        a.his_call = "JA1YWX".into();
        a.tx = 2;
        assert_eq!(a.heard(&decode("JA1AAA JA1YWX -05", 0.0)), None);
        assert_eq!(a.heard(&decode("JA1ZLO JA1BBB -05", 0.0)), None);
        assert_eq!((a.tx, a.his_call.as_str()), (2, "JA1YWX"));
        a.auto = false;
        assert_eq!(a.heard(&decode("JA1ZLO JA1YWX -05", 0.0)), None);
        assert_eq!(a.tx, 2);
    }
}
