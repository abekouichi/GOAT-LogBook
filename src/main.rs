#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Local, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

slint::include_modules!();

const FREQS: [&str; 17] = ["1.9MHz", "3.5MHz", "3.8MHz", "7MHz", "10MHz", "14MHz", "18MHz", "21MHz", "24MHz", "28MHz",
                           "50MHz", "144MHz", "430MHz", "1200MHz", "2400MHz", "5600MHz", "10.1GHz"];
const MODES: [&str; 7] = ["CW", "SSB", "AM", "FM", "FT8", "RTTY", "SSTV"];
const POWERS: [&str; 7] = ["1W", "5W", "10W", "50W", "100W", "200W", "1kW"];
// {callsign} が呼出符号に置換されます。既存コードのURLがある場合は setting.json の api_url で上書きしてください。
const DEFAULT_API: &str = "https://www.tele.soumu.go.jp/musen/list?ST=1&DA=1&DC=1&OF=2&OW=AT&MA={callsign}";

// ---------------------------------------------------------------- データ型
#[derive(Serialize, Deserialize, Clone, Default)]
struct Record {
    #[serde(rename = "日付", default)] date: String,
    #[serde(rename = "開始時刻", default)] start: String,
    #[serde(rename = "終了時刻", default)] end: String,
    #[serde(rename = "コールサイン", default)] call: String,
    #[serde(rename = "RST(S)", default)] rst_s: String,
    #[serde(rename = "RST(R)", default)] rst_r: String,
    #[serde(rename = "周波数", default)] freq: String,
    #[serde(rename = "モード", default)] mode: String,
    #[serde(rename = "出力", default)] power: String,
    #[serde(rename = "名前", default)] name: String,
    #[serde(rename = "QTH", default)] qth: String,
    #[serde(rename = "メモ", default)] memo: String,
    #[serde(rename = "常置場所", default)] loc: String,
    #[serde(rename = "市郡区ナンバー", default)] num: String,
    #[serde(rename = "一括表示", default)] spec: String,
}

impl From<Entry> for Record {
    fn from(e: Entry) -> Self {
        Record {
            date: e.date.to_string(), start: e.t_start.to_string(), end: e.t_end.to_string(),
            call: e.call.to_string(), rst_s: e.rst_s.to_string(), rst_r: e.rst_r.to_string(),
            freq: e.freq.to_string(), mode: e.mode.to_string(), power: e.power.to_string(),
            name: e.name.to_string(), qth: e.qth.to_string(), memo: e.memo.to_string(),
            loc: e.loc.to_string(), num: e.num.to_string(), spec: e.spec.to_string(),
        }
    }
}

impl From<&Record> for Entry {
    fn from(r: &Record) -> Self {
        Entry {
            date: r.date.as_str().into(), t_start: r.start.as_str().into(), t_end: r.end.as_str().into(),
            call: r.call.as_str().into(), rst_s: r.rst_s.as_str().into(), rst_r: r.rst_r.as_str().into(),
            freq: r.freq.as_str().into(), mode: r.mode.as_str().into(), power: r.power.as_str().into(),
            name: r.name.as_str().into(), qth: r.qth.as_str().into(), memo: r.memo.as_str().into(),
            loc: r.loc.as_str().into(), num: r.num.as_str().into(), spec: r.spec.as_str().into(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(default)] last_file: Option<String>,
    #[serde(default = "yes")] use_api: bool,
    #[serde(default = "yes")] sort_asc: bool,
    #[serde(default)] cols: Vec<f32>,
    #[serde(default)] cols_ver: u32,
    #[serde(default)] dark: bool,
    #[serde(default = "yes")] show_clock: bool,
    #[serde(default = "yes")] show_seconds: bool,
    #[serde(default = "yes")] show_popup: bool,
    #[serde(default)] margins: Vec<i32>, // PDFの余白[mm]：上・下・左・右
    #[serde(default)] font_regular: Option<String>,
    #[serde(default)] font_bold: Option<String>,
    #[serde(default)] font_mono: Option<String>,
    #[serde(default)] my_call: String,
    #[serde(default)] col_show: Vec<bool>,
    #[serde(default)] num_file: Option<String>,
    #[serde(default = "default_api")] api_url: String,
}
fn yes() -> bool { true }
fn default_api() -> String { DEFAULT_API.to_string() }
impl Default for Config {
    fn default() -> Self { Config { last_file: None, use_api: true, sort_asc: true, cols: vec![], cols_ver: 0, dark: false, show_clock: true, show_seconds: true, show_popup: true, margins: vec![], font_regular: None, font_bold: None, font_mono: None, my_call: String::new(), col_show: vec![], num_file: None, api_url: default_api() } }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_default()
}
impl Config {
    fn load() -> Self {
        fs::read_to_string(exe_dir().join("setting.json")).ok()
            .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = fs::write(exe_dir().join("setting.json"), t);
        }
    }
}

struct App {
    cfg: Config,
    file: Option<PathBuf>,
    recs: Vec<Record>,
    num: Arc<HashMap<String, String>>,
    last: [String; 3],
    editing: Option<usize>,
    orig: Option<Record>,
    hist_call: String,
    hist_asc: bool,
    list_asc: bool,
    query: String,
    d_num: Option<PathBuf>, // 設定メニューで選んだ（未保存の）市郡区ナンバーのファイル
    doc: Option<Doc>,       // PDF出力画面のデータ
    fonts: Option<Arc<Fonts>>, // PDFに使うフォント（読み込み済み）
}
type Shared = Rc<RefCell<App>>;

// ---------------------------------------------------------------- ファイル入出力
fn load_records(path: &Path) -> Result<Vec<Record>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("読み込みエラー: {e}"))?;
    let t = text.trim();
    if t.is_empty() { return Ok(vec![]); }
    if t.starts_with('[') {
        serde_json::from_str(t).map_err(|e| format!("JSON解析エラー: {e}"))
    } else {
        // 1行1レコード形式にも対応
        t.lines().filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).map_err(|e| format!("JSON解析エラー: {e}"))).collect()
    }
}

fn persist(s: &App) -> Result<(), String> {
    let path = s.file.as_ref().ok_or("ファイルを開くか新規作成してください")?;
    let text = serde_json::to_string_pretty(&s.recs).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| format!("保存エラー: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("保存エラー: {e}"))
}

fn load_num(path: &Path) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Some(serde_json::Value::Object(o)) =
        fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok())
    {
        for (k, v) in o {
            m.insert(k, v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        }
    }
    m
}

fn num_path(cfg: &Config) -> PathBuf {
    cfg.num_file.as_ref().map(PathBuf::from).unwrap_or_else(|| exe_dir().join("shigunku_num.json"))
}

// ---------------------------------------------------------------- 表示用ヘルパ
fn base(call: &str) -> String { call.split('/').next().unwrap_or("").to_uppercase() }

fn log_row(i: usize, r: &Record) -> LogRow {
    LogRow {
        id: i as i32, date: r.date.as_str().into(), t_start: r.start.as_str().into(), t_end: r.end.as_str().into(),
        call: r.call.as_str().into(), rst_s: r.rst_s.as_str().into(), rst_r: r.rst_r.as_str().into(),
        freq: r.freq.as_str().into(), mode: r.mode.as_str().into(), power: r.power.as_str().into(),
        name: r.name.as_str().into(), qth: r.qth.as_str().into(), memo: r.memo.replace('\n', " ").into(),
        memo_full: r.memo.as_str().into(),
        loc: r.loc.as_str().into(), num: r.num.as_str().into(), spec: r.spec.as_str().into(),
    }
}

fn sorted(recs: &[Record], mut idx: Vec<usize>, asc: bool) -> Vec<usize> {
    idx.sort_by(|&a, &b| (&recs[a].date, &recs[a].start).cmp(&(&recs[b].date, &recs[b].start)));
    if !asc { idx.reverse(); }
    idx
}

fn digits(s: &str) -> String { s.chars().filter(|c| c.is_ascii_digit()).collect() }
fn dateish(s: &str) -> bool { s.chars().all(|c| c.is_ascii_digit() || c == '/') }

/// スペース区切りAND検索。「日付A-」「-日付B」「日付A-日付B」で日付範囲。
fn hit(r: &Record, q: &str) -> bool {
    let hay = [&r.date, &r.start, &r.end, &r.call, &r.rst_s, &r.rst_r, &r.freq, &r.mode, &r.power,
               &r.name, &r.qth, &r.memo, &r.loc, &r.num, &r.spec]
        .iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ").to_uppercase();
    let dd = digits(&r.date);
    for tok in q.split_whitespace() {
        if let Some((a, b)) = tok.split_once('-') {
            if dateish(a) && dateish(b) && !(a.is_empty() && b.is_empty()) {
                let (a, b) = (digits(a), digits(b));
                if !a.is_empty() && dd[..a.len().min(dd.len())] < *a.as_str() { return false; }
                if !b.is_empty() && dd[..b.len().min(dd.len())] > *b.as_str() { return false; }
                continue;
            }
        }
        if !hay.contains(&tok.to_uppercase()) { return false; }
    }
    true
}

fn to_model(rows: Vec<LogRow>) -> ModelRc<LogRow> { ModelRc::new(VecModel::from(rows)) }

fn refresh_list(ui: &MainWindow, s: &App) {
    let idx = (0..s.recs.len()).filter(|&i| hit(&s.recs[i], &s.query)).collect();
    let rows = sorted(&s.recs, idx, s.list_asc).into_iter().map(|i| log_row(i, &s.recs[i])).collect();
    let bk = ui.global::<Backend>();
    bk.set_rows(to_model(rows));
    bk.set_total_count(s.recs.len() as i32);
}

fn refresh_hist(ui: &MainWindow, s: &App) {
    let rows = if s.hist_call.is_empty() { vec![] } else {
        let b = base(&s.hist_call);
        let idx = (0..s.recs.len()).filter(|&i| base(&s.recs[i].call) == b).collect();
        sorted(&s.recs, idx, s.hist_asc).into_iter().map(|i| log_row(i, &s.recs[i])).collect()
    };
    ui.global::<Backend>().set_history(to_model(rows));
}

fn status(ui: &MainWindow, msg: &str) { ui.global::<Backend>().set_status(msg.into()); }

/// 入力中の内容があるか（自動入力される日付・時刻・周波数・モード・出力は除く）
fn has_input(ui: &MainWindow) -> bool {
    let f = ui.global::<Form>();
    [f.get_call(), f.get_rst_s(), f.get_rst_r(), f.get_name(), f.get_qth(), f.get_memo()]
        .iter().any(|v| !v.is_empty())
}

/// 一覧・履歴の表示項目（15個）。未設定なら全部表示
/// 旧形式（12個：メモが最後）は、QTHの後ろに常置場所・市郡区ナンバー・一括表示（表示）を挿入して移行
/// 印刷の余白[mm]（上・下・左・右）。未設定なら10mm
fn margins_of(cfg: &Config) -> [i32; 4] {
    if cfg.margins.len() == 4 { [cfg.margins[0], cfg.margins[1], cfg.margins[2], cfg.margins[3]] } else { [10; 4] }
}

fn col_flags(cfg: &Config) -> Vec<bool> {
    match cfg.col_show.len() {
        15 => cfg.col_show.clone(),
        12 => {
            let mut v = cfg.col_show[..11].to_vec();
            v.extend([true; 3]);
            v.push(cfg.col_show[11]);
            v
        }
        _ => vec![true; 15],
    }
}

fn set_col_show(ui: &MainWindow, flags: Vec<bool>) {
    ui.global::<Backend>().set_col_show(ModelRc::new(VecModel::from(flags)));
}

fn read_cols(ui: &MainWindow) -> Vec<f32> {
    let c = ui.global::<Cols>();
    vec![c.get_date(), c.get_t_start(), c.get_t_end(), c.get_call(), c.get_rst_s(), c.get_rst_r(),
         c.get_freq(), c.get_mode(), c.get_power(), c.get_name(), c.get_qth(),
         c.get_loc(), c.get_num(), c.get_spec()]
}

fn apply_cols(ui: &MainWindow, v: &[f32]) {
    if v.len() != 11 && v.len() != 14 { return; } // 11個は旧形式
    let v: Vec<f32> = v.iter().map(|x| x.max(30.0)).collect();
    let c = ui.global::<Cols>();
    c.set_date(v[0]); c.set_t_start(v[1]); c.set_t_end(v[2]); c.set_call(v[3]); c.set_rst_s(v[4]);
    c.set_rst_r(v[5]); c.set_freq(v[6]); c.set_mode(v[7]); c.set_power(v[8]); c.set_name(v[9]); c.set_qth(v[10]);
    if v.len() == 14 { c.set_loc(v[11]); c.set_num(v[12]); c.set_spec(v[13]); }
}

fn bump_focus(ui: &MainWindow) {
    let bk = ui.global::<Backend>();
    bk.set_focus_tick(bk.get_focus_tick() + 1);
}

fn open_file(ui: &MainWindow, s: &mut App, path: PathBuf) -> Result<(), String> {
    s.recs = load_records(&path)?;
    s.cfg.last_file = Some(path.to_string_lossy().to_string());
    s.cfg.save();
    s.query.clear();
    s.hist_call.clear();
    s.editing = None;
    s.orig = None;
    let bk = ui.global::<Backend>();
    bk.set_query("".into());
    bk.set_file_name(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().into());
    s.file = Some(path);
    bk.set_message("".into());
    bk.set_editing(false);
    bk.set_screen(1);
    status(ui, "");
    refresh_list(ui, s);
    refresh_hist(ui, s);
    Ok(())
}

// ---------------------------------------------------------------- 入力値の検査・編集ロジック
fn missing(r: &Record) -> Vec<&'static str> {
    let mut v = vec![];
    for (name, val) in [("日付", &r.date), ("開始時刻", &r.start), ("コールサイン", &r.call), ("RST(S)", &r.rst_s),
                        ("RST(R)", &r.rst_r), ("周波数", &r.freq), ("モード", &r.mode)] {
        if val.trim().is_empty() { v.push(name); }
    }
    v
}

fn collect_form(ui: &MainWindow) -> Record {
    let mut r: Record = ui.global::<Form>().invoke_get().into();
    r.call = r.call.to_uppercase();
    r
}

fn valid_date_prefix(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() > 10 { return false; }
    for (i, &c) in b.iter().enumerate() {
        let ok = if i == 4 || i == 7 { c == b'/' } else { c.is_ascii_digit() };
        if !ok { return false; }
    }
    if b.len() >= 6 && b[5] > b'1' { return false; }
    if b.len() >= 7 && !(1..=12).contains(&((b[5] - b'0') * 10 + (b[6] - b'0'))) { return false; }
    if b.len() >= 9 && b[8] > b'3' { return false; }
    if b.len() == 10 { return NaiveDate::parse_from_str(t, "%Y/%m/%d").is_ok(); }
    true
}

fn valid_time_prefix(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() > 5 { return false; }
    for (i, &c) in b.iter().enumerate() {
        let ok = if i == 2 { c == b':' } else { c.is_ascii_digit() };
        if !ok { return false; }
    }
    if !b.is_empty() && b[0] > b'2' { return false; }
    if b.len() >= 2 && (b[0] - b'0') * 10 + (b[1] - b'0') > 23 { return false; }
    if b.len() >= 4 && b[3] > b'5' { return false; }
    true
}

fn full_time(t: &str) -> bool { t.len() == 5 && valid_time_prefix(t) }

/// 区切り文字（/ や :）を自動で挿入する
fn auto_sep(kind: i32, t: &mut String, c: &mut usize) {
    let (sep, pos): (char, &[usize]) = if kind == 2 { ('/', &[4, 7]) } else { (':', &[2]) };
    for &p in pos {
        if t.len() > p && t.as_bytes()[p].is_ascii_digit() {
            t.insert(p, sep);
            if *c > p { *c += 1; }
        }
    }
}

/// 不正な入力は取り消す（日付 yyyy/mm/dd、時刻 HH:MM）
fn fix_datetime(kind: i32, text: &str, cur: i32) -> (String, i32) {
    let valid = |t: &str| if kind == 2 { valid_date_prefix(t) } else { valid_time_prefix(t) };
    let mut t = text.to_string();
    let mut c = (cur.max(0) as usize).min(t.len());
    auto_sep(kind, &mut t, &mut c);
    if valid(&t) { return (t, c as i32); }
    // 直前に入力された1文字を取り消す
    if let Some(head) = text.get(..(cur.max(0) as usize).min(text.len())) {
        if let Some(ch) = head.chars().next_back() {
            let end = head.len();
            let mut t2 = text.to_string();
            t2.replace_range(end - ch.len_utf8()..end, "");
            let mut c2 = end - ch.len_utf8();
            auto_sep(kind, &mut t2, &mut c2);
            if valid(&t2) { return (t2, c2 as i32); }
        }
    }
    // 貼り付けなど：正しい先頭部分だけ残す
    for end in (0..=text.len()).rev() {
        if text.is_char_boundary(end) && valid(&text[..end]) { return (text[..end].to_string(), end as i32); }
    }
    (String::new(), 0)
}

fn filter(kind: i32, text: &str, cur: i32) -> (String, i32) {
    if kind == 2 || kind == 3 { return fix_datetime(kind, text, cur); }
    let keep = |c: &char| match kind {
        0 => c.is_ascii_alphanumeric() || *c == '/',
        1 => c.is_ascii_digit(),
        _ => true,
    };
    let c = cur.max(0) as usize;
    let head = text.get(..c).unwrap_or(text);
    let nh: String = head.chars().filter(keep).map(|c| c.to_ascii_uppercase()).collect();
    let mut all: String = text.chars().filter(keep).map(|c| if kind == 0 { c.to_ascii_uppercase() } else { c }).collect();
    let mut n = nh.len();
    if kind == 1 { all.truncate(3); n = n.min(all.len()); } // RST は最大3桁
    (all, n as i32)
}

/// 保存時の形式チェック（日付 yyyy/mm/dd、時刻 HH:MM、RST 2〜3桁の数字）
fn invalid(r: &Record) -> Vec<&'static str> {
    let rst = |t: &str| (2..=3).contains(&t.len()) && t.chars().all(|c| c.is_ascii_digit());
    let mut v = vec![];
    if r.date.len() != 10 || NaiveDate::parse_from_str(&r.date, "%Y/%m/%d").is_err() { v.push("日付"); }
    if !full_time(&r.start) { v.push("開始時刻"); }
    if !r.end.is_empty() && !full_time(&r.end) { v.push("終了時刻"); }
    if !rst(&r.rst_s) { v.push("RST(S)"); }
    if !rst(&r.rst_r) { v.push("RST(R)"); }
    v
}

fn shift_months(d: NaiveDate, m: i32) -> NaiveDate {
    let r = if m >= 0 { d.checked_add_months(Months::new(m as u32)) } else { d.checked_sub_months(Months::new((-m) as u32)) };
    r.unwrap_or(d)
}

/// 日付(2)・時刻(3)の↑↓。カーソル位置の単位を増減し、カーソル位置は変えない。
fn step(kind: i32, text: &str, cur: i32, dir: i32) -> (String, i32) {
    let c = cur.max(0) as usize;
    let same = (text.to_string(), cur);
    match kind {
        2 => {
            let Ok(d) = NaiveDate::parse_from_str(text, "%Y/%m/%d") else { return same };
            let n = if c <= 4 { shift_months(d, 12 * dir) }
                    else if c <= 7 { shift_months(d, dir) }
                    else { d + chrono::Duration::days(dir as i64) };
            (n.format("%Y/%m/%d").to_string(), cur)
        }
        3 => {
            let Some((h, m)) = text.split_once(':') else { return same };
            let (Ok(h), Ok(m)) = (h.parse::<i32>(), m.parse::<i32>()) else { return same };
            // 分の増減は時に繰り上がり・繰り下がり（0:00 と 23:59 の間は循環）
            let delta = if c <= 2 { 60 * dir } else { dir };
            let t = (h * 60 + m + delta).rem_euclid(1440);
            (format!("{:02}:{:02}", t / 60, t % 60), cur)
        }
        _ => same,
    }
}

fn options(kind: i32) -> Vec<String> {
    match kind {
        0 => FREQS.iter().map(|f| f.to_string()).collect(),
        1 => MODES.iter().map(|s| s.to_string()).collect(),
        _ => POWERS.iter().map(|s| s.to_string()).collect(),
    }
}

fn cycle(kind: i32, cur: &str, dir: i32) -> String {
    let o = options(kind);
    let n = o.len() as i32;
    match o.iter().position(|x| x == cur) {
        Some(p) => o[(p as i32 + dir).rem_euclid(n) as usize].clone(),
        None => if dir > 0 { o[0].clone() } else { o[(n - 1) as usize].clone() },
    }
}

// ---------------------------------------------------------------- 総務省 無線局等情報検索 Web-API
fn collect(v: &serde_json::Value, key: &str, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                if k == key {
                    if let Some(s) = x.as_str() { if !s.is_empty() { out.push(s.to_string()); } }
                }
                collect(x, key, out);
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect(x, key, out)),
        _ => {}
    }
}

/// "musen"（局ごとの配列）の要素を集める
fn find_records<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) -> bool {
    match v {
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                if k == "musen" {
                    if let serde_json::Value::Array(a) = x { out.extend(a.iter()); return true; }
                }
                if find_records(x, out) { return true; }
            }
            false
        }
        serde_json::Value::Array(a) => a.iter().any(|x| find_records(x, out)),
        _ => false,
    }
}

/// いずれかの文字列の値が、呼出符号と完全に一致するか
fn has_exact(v: &serde_json::Value, call: &str) -> bool {
    match v {
        serde_json::Value::String(s) => s.trim().eq_ignore_ascii_case(call),
        serde_json::Value::Object(o) => o.values().any(|x| has_exact(x, call)),
        serde_json::Value::Array(a) => a.iter().any(|x| has_exact(x, call)),
        _ => false,
    }
}

fn dedup(v: Vec<String>) -> Vec<String> {
    let mut o: Vec<String> = vec![];
    for x in v { if !o.contains(&x) { o.push(x); } }
    o
}

/// 戻り値: (常置場所, 市郡区ナンバー, 一括表示)
fn fetch(tpl: &str, call: &str, num: &HashMap<String, String>, paren: bool) -> Result<(String, String, String), String> {
    let mut url = tpl.replace("{callsign}", &base(call));
    if !url.contains("SC=") { url.push_str("&SC=1"); } // スタートカウント必須（保存済みの旧URLにも対応）
    let body = ureq::get(&url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) logbook/0.1")
        .timeout(std::time::Duration::from_secs(10)).call()
        .map_err(|e| match e {
            ureq::Error::Status(c, r) => format!("HTTP {c}: {}", r.into_string().unwrap_or_default().chars().take(100).collect::<String>()),
            other => format!("通信エラー: {other}"),
        })?
        .into_string().map_err(|e| format!("取得エラー: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("解析エラー: {e}"))?;
    let mut errs = vec![];
    collect(&v, "errMsg", &mut errs);
    if !errs.is_empty() { return Err(format!("API: {}", errs.join(" "))); }
    // 部分一致を排除：識別信号（呼出符号）が完全一致する局だけを使う
    let target = base(call);
    let mut recs: Vec<&serde_json::Value> = vec![];
    let hits: Vec<&serde_json::Value> = if find_records(&v, &mut recs) {
        recs.into_iter().filter(|r| has_exact(r, &target)).collect()
    } else {
        if has_exact(&v, &target) { vec![&v] } else { vec![] }
    };
    let (mut locs, mut specs) = (vec![], vec![]);
    for r in hits {
        collect(r, "radioEuipmentLocation", &mut locs);
        collect(r, "radioSpec1", &mut specs);
    }
    let (locs, specs) = (dedup(locs), dedup(specs));
    if locs.is_empty() && specs.is_empty() {
        let nf = "Not found".to_string();
        return Ok((nf.clone(), nf.clone(), nf));
    }
    let nums = dedup(locs.iter().map(|l| num.get(l).cloned().unwrap_or_else(|| "Not found".into())).collect());
    let wrap = |s: String| if paren { format!("({s})") } else { s };
    Ok((wrap(locs.join(",")), wrap(nums.join(",")), specs.join(",")))
}

fn start_lookup(ui: &MainWindow, tpl: String, call: String, num: Arc<HashMap<String, String>>) {
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let res = fetch(&tpl, &call, &num, call.contains('/'));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let f = ui.global::<Form>();
                if f.get_call().as_str() != call { return; } // 入力が変わっていたら捨てる
                match res {
                    Ok((l, n, s)) => f.invoke_set_info(l.into(), n.into(), s.into()),
                    Err(e) => {
                        f.invoke_set_info("取得エラー".into(), "".into(), "".into());
                        status(&ui, &e);
                    }
                }
            }
        });
    });
}

// ---------------------------------------------------------------- 印刷データ
#[derive(Clone)]
struct PrintJob { title: String, info: String, heads: Vec<String>, rows: Vec<Vec<String>>, mono: Vec<bool>, margins: [i32; 4] }

/// リスト（現在の検索・並び順）から、表示項目の選択に従った印刷データを作る
fn build_job(s: &App) -> Result<PrintJob, String> {
    if s.file.is_none() { return Err("ファイルを開いてからPDF出力してください".into()); }
    let idx = (0..s.recs.len()).filter(|&i| hit(&s.recs[i], &s.query)).collect();
    let idx = sorted(&s.recs, idx, s.list_asc);
    if idx.is_empty() { return Err("PDFにするデータがありません".into()); }
    let flags = col_flags(&s.cfg);
    let heads = ["日付", "開始時刻", "終了時刻", "コールサイン", "RST(S)", "RST(R)", "周波数", "モード", "出力",
                 "名前", "QTH", "常置場所", "市郡区ナンバー", "一括表示", "メモ"];
    let cols: Vec<usize> = (0..15).filter(|&c| flags[c]).collect();
    if cols.is_empty() { return Err("表示する項目がありません。設定で表示項目を選んでください".into()); }
    let cell = |r: &Record, c: usize| -> String {
        match c {
            0 => r.date.clone(), 1 => r.start.clone(), 2 => r.end.clone(), 3 => r.call.clone(),
            4 => r.rst_s.clone(), 5 => r.rst_r.clone(), 6 => r.freq.clone(), 7 => r.mode.clone(),
            8 => r.power.clone(), 9 => r.name.clone(), 10 => r.qth.clone(), 11 => r.loc.clone(),
            12 => r.num.clone(), 13 => r.spec.clone(), _ => r.memo.clone(),
        }
    };
    let hv: Vec<String> = cols.iter().map(|&c| heads[c].to_string()).collect();
    let rows: Vec<Vec<String>> = idx.iter()
        .map(|&i| cols.iter().map(|&c| cell(&s.recs[i], c).replace(['\r', '\n'], " ")).collect())
        .collect();
    let file = s.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let title = format!("GOAT LogBook　{}", file);
    let mut info = format!("出力日時: {}　／　{}件", Local::now().format("%Y/%m/%d %H:%M"), rows.len());
    if !s.cfg.my_call.is_empty() { info = format!("{}　／　{}", s.cfg.my_call, info); }
    if !s.query.is_empty() { info.push_str(&format!("　／　検索: {}", s.query)); }
    let mono: Vec<bool> = cols.iter().map(|&c| c == 3).collect(); // コールサインは等幅（画面と同じ Consolas）
    Ok(PrintJob { title, info, heads: hv, rows, mono, margins: margins_of(&s.cfg) })
}

// ---------------------------------------------------------------- PDF出力（プレビューと共通のレイアウト）
// 印刷はやめて、PDFだけを作る。レイアウトを1回だけ計算し、PDFとプレビュー画像の両方に同じ描画命令を使う。
// OSに依存しないので、Windows・Linuxのどちらでも動く。
const PAGE_W: f32 = 297.0; // A4横 [mm]
const PAGE_H: f32 = 210.0;

#[derive(Clone, Copy)]
enum FontKind { Regular, Bold, Mono }

/// 描画命令。座標は mm、原点は用紙の左上。Text の y はベースライン
enum Cmd {
    Rect { x: f32, y: f32, w: f32, h: f32, fill: Option<f32>, stroke: bool },
    Text { x: f32, y: f32, size: f32, font: FontKind, text: String },
}

struct Fonts {
    regular: fontdue::Font, bold: fontdue::Font, mono: fontdue::Font,
    d_regular: Vec<u8>, d_bold: Vec<u8>, d_mono: Vec<u8>,
}
impl Fonts {
    fn get(&self, k: FontKind) -> &fontdue::Font {
        match k { FontKind::Regular => &self.regular, FontKind::Bold => &self.bold, FontKind::Mono => &self.mono }
    }
    fn data(&self, k: FontKind) -> &Vec<u8> {
        match k { FontKind::Regular => &self.d_regular, FontKind::Bold => &self.d_bold, FontKind::Mono => &self.d_mono }
    }
}

/// PDF出力画面で使うデータ（元データ・フォント・描画命令）
struct Doc { job: PrintJob, fonts: Arc<Fonts>, pages: Vec<Vec<Cmd>> }

/// フォントを読み込む。実行ファイルと同じ場所の fonts フォルダ、または setting.json で指定した場所から探す
fn load_fonts(cfg: &Config) -> Result<Fonts, String> {
    let dir = exe_dir().join("fonts");
    let find = |custom: &Option<String>, names: &[&str]| -> Option<PathBuf> {
        if let Some(p) = custom {
            let p = PathBuf::from(p);
            if p.exists() { return Some(p); }
        }
        names.iter().map(|n| dir.join(n)).find(|p| p.exists())
    };
    let reg = find(&cfg.font_regular, &["NotoSansJP-Regular.ttf", "NotoSansJP-VariableFont_wght.ttf", "NotoSansJP.ttf"])
        .ok_or_else(|| format!("フォントが見つかりません。{} に NotoSansJP-Regular.ttf を置いてください", dir.display()))?;
    let d_regular = fs::read(&reg).map_err(|e| format!("フォントを読めません: {e}"))?;
    let d_bold = find(&cfg.font_bold, &["NotoSansJP-Bold.ttf"])
        .and_then(|p| fs::read(p).ok()).unwrap_or_else(|| d_regular.clone());
    let d_mono = find(&cfg.font_mono, &["NotoSansMono-Regular.ttf", "NotoSansMono-VariableFont_wdth,wght.ttf", "NotoSansMono.ttf"])
        .and_then(|p| fs::read(p).ok()).unwrap_or_else(|| d_regular.clone());
    let parse = |d: &Vec<u8>| fontdue::Font::from_bytes(d.as_slice(), fontdue::FontSettings::default())
        .map_err(|e| format!("フォントを解析できません: {e}"));
    let regular = parse(&d_regular)?;
    let bold = parse(&d_bold)?;
    let mono = parse(&d_mono)?;
    Ok(Fonts { regular, bold, mono, d_regular, d_bold, d_mono })
}

fn text_w(f: &fontdue::Font, t: &str, pt: f32) -> f32 {
    t.chars().map(|c| f.metrics(c, pt).advance_width).sum::<f32>() * 25.4 / 72.0
}

/// 幅 max_mm に収まるように、文字列の後ろを切る
fn fit_text(f: &fontdue::Font, t: &str, pt: f32, max_mm: f32) -> String {
    let mut w = 0.0;
    let mut out = String::new();
    for c in t.chars() {
        let a = f.metrics(c, pt).advance_width * 25.4 / 72.0;
        if w + a > max_mm { break; }
        w += a;
        out.push(c);
    }
    out
}

/// 余白（上・下・左・右 [mm]）に従って、全ページの描画命令を作る
fn build_pages(job: &PrintJob, fonts: &Fonts) -> Vec<Vec<Cmd>> {
    let (mt, mb, ml, mr) = (job.margins[0] as f32, job.margins[1] as f32, job.margins[2] as f32, job.margins[3] as f32);
    let (body, title) = (9.0_f32, 13.0_f32);
    let mm_pt = |pt: f32| pt * 25.4 / 72.0;
    let (pad, vpad) = (1.5_f32, 0.8_f32);
    let rh = mm_pt(body) * 1.35 + 2.0 * vpad;
    let title_h = mm_pt(title) * 1.35 + 1.5;
    let info_h = mm_pt(body) * 1.35 + 3.0;
    let foot_h = mm_pt(body) * 1.35 + 2.0;
    let asc = |pt: f32| mm_pt(pt); // 行の上からベースラインまで

    // 列幅：内容の最大幅（上限70mm）。用紙に収まらないときは全体を縮める
    let n = job.heads.len();
    let mut cw: Vec<f32> = (0..n).map(|c| {
        let mut w = text_w(&fonts.bold, &job.heads[c], body);
        let f = if job.mono[c] { &fonts.mono } else { &fonts.regular };
        for r in &job.rows { w = w.max(text_w(f, &r[c], body)); if w >= 70.0 { break; } }
        w.min(70.0) + 2.0 * pad
    }).collect();
    let avail = (PAGE_W - ml - mr).max(30.0);
    let total: f32 = cw.iter().sum();
    if total > avail { for w in cw.iter_mut() { *w = *w * avail / total; } }

    let body_h = PAGE_H - mt - mb - title_h - info_h - foot_h - rh;
    let rpp = ((body_h / rh).floor() as usize).max(1);
    let pages = ((job.rows.len() + rpp - 1) / rpp).max(1);

    let push_row = |cmds: &mut Vec<Cmd>, y: f32, cells: &[String], header: bool| {
        let mut x = ml;
        for (c, v) in cells.iter().enumerate() {
            let w = cw[c];
            cmds.push(Cmd::Rect { x, y, w, h: rh, fill: if header { Some(0.91) } else { None }, stroke: true });
            let (fk, f) = if header { (FontKind::Bold, &fonts.bold) }
                          else if job.mono[c] { (FontKind::Mono, &fonts.mono) }
                          else { (FontKind::Regular, &fonts.regular) };
            let t = fit_text(f, v, body, (w - 2.0 * pad).max(0.0));
            cmds.push(Cmd::Text { x: x + pad, y: y + vpad + asc(body), size: body, font: fk, text: t });
            x += w;
        }
    };

    let mut out = Vec::with_capacity(pages);
    for page in 0..pages {
        let mut cmds: Vec<Cmd> = vec![];
        let mut y = mt;
        cmds.push(Cmd::Text { x: ml, y: y + asc(title), size: title, font: FontKind::Bold,
                              text: fit_text(&fonts.bold, &job.title, title, PAGE_W - ml - mr) });
        y += title_h;
        cmds.push(Cmd::Text { x: ml, y: y + asc(body), size: body, font: FontKind::Regular,
                              text: fit_text(&fonts.regular, &job.info, body, PAGE_W - ml - mr) });
        y += info_h;
        push_row(&mut cmds, y, &job.heads, true);
        y += rh;
        let from = page * rpp;
        let to = ((page + 1) * rpp).min(job.rows.len());
        for r in &job.rows[from..to] {
            push_row(&mut cmds, y, r, false);
            y += rh;
        }
        let foot = format!("{} / {}", page + 1, pages);
        let fw = text_w(&fonts.regular, &foot, body);
        cmds.push(Cmd::Text { x: (ml + PAGE_W - mr) / 2.0 - fw / 2.0, y: PAGE_H - mb - mm_pt(body) * 0.3,
                              size: body, font: FontKind::Regular, text: foot });
        out.push(cmds);
    }
    out
}

fn make_doc(job: PrintJob, fonts: Arc<Fonts>) -> Doc {
    let pages = build_pages(&job, &fonts);
    Doc { job, fonts, pages }
}

/// PDFファイルに書き出す。使ったフォントは、PDFに埋め込まれる
fn write_pdf(doc: &Doc, path: &Path) -> Result<(), String> {
    use printpdf::*;
    use std::io::{BufWriter, Cursor};
    let (pdf, p0, l0) = PdfDocument::new(doc.job.title.as_str(), Mm(PAGE_W as _), Mm(PAGE_H as _), "layer");
    let load = |k: FontKind| pdf.add_external_font(Cursor::new(doc.fonts.data(k).clone())).map_err(|e| format!("フォントをPDFに埋め込めません: {e}"));
    let f_reg = load(FontKind::Regular)?;
    let f_bold = load(FontKind::Bold)?;
    let f_mono = load(FontKind::Mono)?;
    for (i, cmds) in doc.pages.iter().enumerate() {
        let layer = if i == 0 {
            pdf.get_page(p0).get_layer(l0)
        } else {
            let (pi, li) = pdf.add_page(Mm(PAGE_W as _), Mm(PAGE_H as _), "layer");
            pdf.get_page(pi).get_layer(li)
        };
        for c in cmds {
            match c {
                Cmd::Rect { x, y, w, h, fill, stroke } => {
                    let mk = || Rect::new(Mm(*x as _), Mm((PAGE_H - y - h) as _), Mm((x + w) as _), Mm((PAGE_H - y) as _));
                    if let Some(g) = fill {
                        layer.set_fill_color(Color::Greyscale(Greyscale::new(*g as _, None)));
                        layer.add_rect(mk().with_mode(PaintMode::Fill));
                    }
                    if *stroke {
                        layer.set_outline_color(Color::Greyscale(Greyscale::new(0.4, None)));
                        layer.set_outline_thickness(0.3);
                        layer.add_rect(mk().with_mode(PaintMode::Stroke));
                    }
                }
                Cmd::Text { x, y, size, font, text } => {
                    let f = match font { FontKind::Regular => &f_reg, FontKind::Bold => &f_bold, FontKind::Mono => &f_mono };
                    layer.set_fill_color(Color::Greyscale(Greyscale::new(0.0, None)));
                    layer.use_text(text.as_str(), *size as _, Mm(*x as _), Mm((PAGE_H - y) as _), f);
                }
            }
        }
    }
    let file = fs::File::create(path).map_err(|e| format!("PDFを作れません: {e}"))?;
    pdf.save(&mut BufWriter::new(file)).map_err(|e| format!("PDFを保存できません: {e}"))
}

fn set_px(buf: &mut [u8], w: i32, h: i32, x: i32, y: i32, v: u8) {
    if x < 0 || y < 0 || x >= w || y >= h { return; }
    let i = ((y * w + x) * 4) as usize;
    buf[i] = v; buf[i + 1] = v; buf[i + 2] = v;
}

/// 指定ページを画像（RGBA）に描く。戻り値: (幅, 高さ, 画素)
fn render_page(doc: &Doc, page: usize) -> (u32, u32, Vec<u8>) {
    let dpi = 100.0_f32;
    let w = (PAGE_W / 25.4 * dpi) as i32;
    let h = (PAGE_H / 25.4 * dpi) as i32;
    let mut px = vec![255u8; (w * h * 4) as usize];
    let mm = |v: f32| v / 25.4 * dpi;
    for c in &doc.pages[page] {
        match c {
            Cmd::Rect { x, y, w: rw, h: rh, fill, stroke } => {
                let (x0, y0, x1, y1) = (mm(*x) as i32, mm(*y) as i32, mm(x + rw) as i32, mm(y + rh) as i32);
                if let Some(g) = fill {
                    let v = (g * 255.0) as u8;
                    for yy in y0..y1 { for xx in x0..x1 { set_px(&mut px, w, h, xx, yy, v); } }
                }
                if *stroke {
                    for xx in x0..=x1 { set_px(&mut px, w, h, xx, y0, 0x66); set_px(&mut px, w, h, xx, y1, 0x66); }
                    for yy in y0..=y1 { set_px(&mut px, w, h, x0, yy, 0x66); set_px(&mut px, w, h, x1, yy, 0x66); }
                }
            }
            Cmd::Text { x, y, size, font, text } => {
                let f = doc.fonts.get(*font);
                let psize = size / 72.0 * dpi;
                let mut pen = mm(*x);
                let base = mm(*y);
                for ch in text.chars() {
                    let (m, bmp) = f.rasterize(ch, psize);
                    let gx = (pen + m.xmin as f32).round() as i32;
                    let gy = (base - m.height as f32 - m.ymin as f32).round() as i32;
                    for r in 0..m.height {
                        for cc in 0..m.width {
                            let a = bmp[r * m.width + cc] as u32;
                            if a == 0 { continue; }
                            let (xx, yy) = (gx + cc as i32, gy + r as i32);
                            if xx < 0 || yy < 0 || xx >= w || yy >= h { continue; }
                            let i = ((yy * w + xx) * 4) as usize;
                            for k in 0..3 { px[i + k] = ((px[i + k] as u32) * (255 - a) / 255) as u8; }
                        }
                    }
                    pen += m.advance_width;
                }
            }
        }
    }
    (w as u32, h as u32, px)
}

/// プレビュー画面に、指定ページを描いて表示する
fn show_preview(ui: &MainWindow, doc: &Doc, page: usize) -> Result<(), String> {
    let p = page.min(doc.pages.len().saturating_sub(1));
    let (w, h, rgba) = render_page(doc, p);
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(w, h);
    buf.make_mut_bytes().copy_from_slice(&rgba);
    let bk = ui.global::<Backend>();
    bk.set_preview_image(slint::Image::from_rgba8(buf));
    bk.set_preview_page(p as i32);
    bk.set_preview_pages(doc.pages.len() as i32);
    bk.set_preview_open(true);
    Ok(())
}

/// 「PDF出力…」：印刷データとフォントを用意して、プレビュー画面を開く
fn open_pdf_screen(ui: &MainWindow, s: &mut App) -> Result<(), String> {
    let job = build_job(s)?;
    let fonts = match &s.fonts {
        Some(f) => f.clone(),
        None => { let f = Arc::new(load_fonts(&s.cfg)?); s.fonts = Some(f.clone()); f }
    };
    let m = job.margins;
    let bk = ui.global::<Backend>();
    bk.set_m_top(m[0]); bk.set_m_bottom(m[1]); bk.set_m_left(m[2]); bk.set_m_right(m[3]);
    let doc = make_doc(job, fonts);
    show_preview(ui, &doc, 0)?;
    s.doc = Some(doc);
    Ok(())
}

// ---------------------------------------------------------------- 起動時にウィンドウを作業領域いっぱいに
#[cfg(windows)]
fn fit_to_screen(ui: &MainWindow) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rc as *mut RECT as *mut core::ffi::c_void, 0)
    };
    if ok == 0 {
        ui.window().set_maximized(true);
        return;
    }
    let dpi = (96.0 * ui.window().scale_factor()).round() as u32;
    let m = |i: i32| unsafe { GetSystemMetricsForDpi(i, dpi) };
    let fx = m(SM_CXSIZEFRAME) + m(SM_CXPADDEDBORDER);
    let fy = m(SM_CYSIZEFRAME) + m(SM_CXPADDEDBORDER);
    let w = (rc.right - rc.left - 2 * fx).max(600);
    let h = (rc.bottom - rc.top - m(SM_CYCAPTION) - 2 * fy).max(400);
    ui.window().set_position(slint::PhysicalPosition::new(rc.left, rc.top));
    ui.window().set_size(slint::PhysicalSize::new(w as u32, h as u32));
}

// ---------------------------------------------------------------- main
fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let mut cfg = Config::load();
    // 一時的に1.5倍にして保存した列幅（cols_ver == 1）を元の大きさに戻す
    if cfg.cols_ver < 2 {
        if cfg.cols_ver == 1 && !cfg.cols.is_empty() {
            cfg.cols = cfg.cols.iter().map(|x| x / 1.5).collect();
        }
        cfg.cols_ver = 2;
        cfg.save();
    }
    let num = Arc::new(load_num(&num_path(&cfg)));
    let asc = cfg.sort_asc;
    let app: Shared = Rc::new(RefCell::new(App {
        cfg, file: None, recs: vec![], num, last: Default::default(), editing: None, orig: None,
        hist_call: String::new(), hist_asc: asc, list_asc: asc, query: String::new(), d_num: None, doc: None, fonts: None,
    }));

    let bk = ui.global::<Backend>();
    let strs = |v: Vec<String>| ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>()));
    bk.set_freq_opts(strs(options(0)));
    bk.set_mode_opts(strs(options(1)));
    bk.set_power_opts(strs(options(2)));
    bk.set_use_api(app.borrow().cfg.use_api);
    bk.set_sort_asc(asc);
    ui.global::<Theme>().set_dark(app.borrow().cfg.dark);
    bk.set_show_clock(app.borrow().cfg.show_clock);
    bk.set_show_seconds(app.borrow().cfg.show_seconds);
    bk.set_show_popup(app.borrow().cfg.show_popup);
    bk.set_my_call(app.borrow().cfg.my_call.as_str().into());
    apply_cols(&ui, &app.borrow().cfg.cols);
    set_col_show(&ui, col_flags(&app.borrow().cfg));

    // 純粋な補助コールバック
    bk.on_now(|k| {
        let n = Local::now();
        (match k { 0 => n.format("%Y/%m/%d"), 2 => n.format("%H:%M:%S"), _ => n.format("%H:%M") }).to_string().into()
    });
    bk.on_step(|k, t, c, d| { let (t, c) = step(k, &t, c, d); Edit { text: t.into(), cursor: c } });
    bk.on_filter(|k, t, c| { let (t, c) = filter(k, &t, c); Edit { text: t.into(), cursor: c } });
    bk.on_cycle(|k, t, d| cycle(k, &t, d).into());
    bk.on_byte_len(|t| t.len() as i32);
    { let a = app.clone();
      bk.on_last(move |k| a.try_borrow().map(|s| s.last[k as usize].clone()).unwrap_or_default().into()); }

    // 画面遷移
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_goto(move |n| {
          let ui = w.unwrap(); let mut s = a.borrow_mut(); let bk = ui.global::<Backend>();
          if s.editing.is_some() {
              status(&ui, "編集中は他のモードに移れません。「保存して終了」「元に戻す」「保存せず終了」「削除」のいずれかで終了してください");
              return;
          }
          if n == 1 {
              if has_input(&ui) {
                  status(&ui, "入力中の内容があります。「保存して次入力」するか「クリア」してください");
                  return;
              }
              ui.global::<Form>().invoke_clear();
              s.hist_call.clear();
              refresh_hist(&ui, &s);
              s.query.clear();
              bk.set_query("".into());
              refresh_list(&ui, &s);
          }
          bk.set_screen(n);
          status(&ui, "");
          bump_focus(&ui);
      }); }

    // 設定メニュー：開くとき、現在の設定を編集用の値にコピーする
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_settings_open(move || {
          let ui = w.unwrap(); let mut s = a.borrow_mut(); let bk = ui.global::<Backend>();
          bk.set_d_use_api(s.cfg.use_api);
          bk.set_d_dark(s.cfg.dark);
          bk.set_d_clock(s.cfg.show_clock);
          bk.set_d_sec(s.cfg.show_seconds);
          bk.set_d_popup(s.cfg.show_popup);
          bk.set_d_call(s.cfg.my_call.as_str().into());
          bk.set_d_col_show(ModelRc::new(VecModel::from(col_flags(&s.cfg))));
          s.d_num = None;
      }); }

    // 設定メニュー「保存」：編集用の値を反映して保存する（「キャンセル」は何もしない）
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_settings_save(move || {
          let ui = w.unwrap(); let mut s = a.borrow_mut(); let bk = ui.global::<Backend>();
          s.cfg.use_api = bk.get_d_use_api();
          bk.set_use_api(s.cfg.use_api);
          s.cfg.dark = bk.get_d_dark();
          ui.global::<Theme>().set_dark(s.cfg.dark);
          s.cfg.show_clock = bk.get_d_clock();
          bk.set_show_clock(s.cfg.show_clock);
          s.cfg.show_seconds = bk.get_d_sec();
          bk.set_show_seconds(s.cfg.show_seconds);
          s.cfg.show_popup = bk.get_d_popup();
          bk.set_show_popup(s.cfg.show_popup);
          s.cfg.my_call = bk.get_d_call().to_uppercase();
          bk.set_my_call(s.cfg.my_call.as_str().into());
          let f: Vec<bool> = bk.get_d_col_show().iter().collect();
          if f.len() == 15 {
              s.cfg.col_show = f.clone();
              set_col_show(&ui, f);
          }
          if let Some(p) = s.d_num.take() {
              s.num = Arc::new(load_num(&p));
              s.cfg.num_file = Some(p.to_string_lossy().to_string());
          }
          s.cfg.save();
      }); }

    // 印刷プレビュー：ページ移動／印刷
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_preview_goto(move |p| {
          let ui = w.unwrap(); let s = a.borrow();
          if let Some(doc) = &s.doc {
              if let Err(e) = show_preview(&ui, doc, p.max(0) as usize) { status(&ui, &e); }
          }
      }); }
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_margins_changed(move || {
          let ui = w.unwrap(); let mut s = a.borrow_mut(); let bk = ui.global::<Backend>();
          let m = [bk.get_m_top(), bk.get_m_bottom(), bk.get_m_left(), bk.get_m_right()];
          s.cfg.margins = m.to_vec();
          s.cfg.save();
          if let Some(doc) = s.doc.as_mut() {
              doc.job.margins = m;
              doc.pages = build_pages(&doc.job, &doc.fonts);
          }
          if let Some(doc) = &s.doc {
              if let Err(e) = show_preview(&ui, doc, bk.get_preview_page().max(0) as usize) { status(&ui, &e); }
          }
      }); }
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_save_pdf(move || {
          let ui = w.unwrap(); let s = a.borrow();
          if let Some(doc) = &s.doc {
              if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name("logbook.pdf").save_file() {
                  match write_pdf(doc, &p) {
                      Ok(()) => status(&ui, &format!("PDFを保存しました: {}", p.display())),
                      Err(e) => status(&ui, &e),
                  }
              }
          }
      }); }

    // 入力のクリア（日付欄へ）
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_clear_form(move || {
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          ui.global::<Form>().invoke_clear();
          s.hist_call.clear();
          refresh_hist(&ui, &s);
          status(&ui, "");
          bump_focus(&ui);
      }); }

    // 列幅の保存（ドラッグ終了時）
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_cols_changed(move || {
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          s.cfg.cols = read_cols(&ui);
          s.cfg.cols_ver = 2;
          s.cfg.save();
      }); }

    // メニュー
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_menu(move |m, i| {
          let ui = w.unwrap();
          match (m, i) {
              (0, 0) => {
                  if let Some(p) = rfd::FileDialog::new().add_filter("logbook", &["logbook"]).pick_file() {
                      if let Err(e) = open_file(&ui, &mut a.borrow_mut(), p) { status(&ui, &e); }
                  }
              }
              (0, 1) => {
                  if let Some(mut p) = rfd::FileDialog::new().add_filter("logbook", &["logbook"])
                      .set_file_name("log.logbook").save_file() {
                      if p.extension().is_none() { p.set_extension("logbook"); }
                      if !p.exists() {
                          if let Err(e) = fs::write(&p, "[]") { status(&ui, &format!("作成エラー: {e}")); return; }
                      }
                      if let Err(e) = open_file(&ui, &mut a.borrow_mut(), p) { status(&ui, &e); }
                  }
              }
              (0, 2) => {
                  let mut s = a.borrow_mut();
                  s.file = None; s.recs.clear(); s.editing = None; s.orig = None; s.hist_call.clear();
                  s.cfg.last_file = None; s.cfg.save();
                  let bk = ui.global::<Backend>();
                  bk.set_file_name("".into());
                  bk.set_message("「ファイル」メニューから、ファイルを開くか新規作成してください".into());
                  bk.set_editing(false);
                  bk.set_screen(1);
                  refresh_list(&ui, &s); refresh_hist(&ui, &s);
              }
              (0, 3) => {
                  let mut s = a.borrow_mut();
                  match open_pdf_screen(&ui, &mut s) { Ok(()) => status(&ui, ""), Err(e) => status(&ui, &e) }
              }
              (0, 5) => { let _ = slint::quit_event_loop(); }
              (2, 0) | (2, 1) => { ui.global::<Backend>().set_d_use_api(i == 0); }
              (2, 3) => {
                  if let Some(p) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                      a.borrow_mut().d_num = Some(p);
                  }
              }
              (2, 6) => { let bk = ui.global::<Backend>(); bk.set_d_dark(!bk.get_d_dark()); }
              (2, 7) => { let bk = ui.global::<Backend>(); bk.set_d_clock(!bk.get_d_clock()); }
              (2, 8) => { let bk = ui.global::<Backend>(); bk.set_d_sec(!bk.get_d_sec()); }
              (2, 9) => { let bk = ui.global::<Backend>(); bk.set_d_popup(!bk.get_d_popup()); }
              (3, i) if (0..15).contains(&i) => {
                  let bk = ui.global::<Backend>();
                  let mut f: Vec<bool> = bk.get_d_col_show().iter().collect();
                  if f.len() == 15 {
                      f[i as usize] = !f[i as usize];
                      bk.set_d_col_show(ModelRc::new(VecModel::from(f)));
                  }
              }
              _ => {}
          }
      }); }

    // コールサイン確定（Enter / Tab）→ 履歴更新 + API検索
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_lookup(move |call| {
          let call = call.to_uppercase();
          if call.is_empty() { return; }
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          s.hist_call = call.clone();
          refresh_hist(&ui, &s);
          if s.cfg.use_api { start_lookup(&ui, s.cfg.api_url.clone(), call, s.num.clone()); }
      }); }

    // 保存（入力画面）
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_save(move || {
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          if s.file.is_none() { status(&ui, "ファイルを開くか新規作成してください"); return; }
          let rec = collect_form(&ui);
          let m = missing(&rec);
          if !m.is_empty() { status(&ui, &format!("未入力: {}", m.join("・"))); return; }
          let bad = invalid(&rec);
          if !bad.is_empty() { status(&ui, &format!("形式が正しくありません: {}", bad.join("・"))); return; }
          s.recs.push(rec.clone());
          if let Err(e) = persist(&s) { s.recs.pop(); status(&ui, &e); return; }
          for (i, v) in [&rec.freq, &rec.mode, &rec.power].into_iter().enumerate() {
              if !v.is_empty() { s.last[i] = v.clone(); }
          }
          ui.global::<Form>().invoke_clear();
          s.hist_call.clear();
          refresh_hist(&ui, &s);
          status(&ui, "保存しました");
          bump_focus(&ui);
      }); }

    // 一覧 → 編集
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_edit_row(move |id| {
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          let i = id as usize;
          if i >= s.recs.len() { return; }
          s.editing = Some(i);
          s.orig = Some(s.recs[i].clone());
          ui.global::<Form>().invoke_put((&s.recs[i]).into());
          let bk = ui.global::<Backend>();
          bk.set_editing(true);
          bk.set_screen(0);
          status(&ui, "");
          bump_focus(&ui);
      }); }

    // 編集の終了操作
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_edit_end(move |k| {
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          let Some(i) = s.editing else { return };
          match k {
              0 => {
                  let rec = collect_form(&ui);
                  let m = missing(&rec);
                  if !m.is_empty() { status(&ui, &format!("未入力: {}", m.join("・"))); return; }
                  let bad = invalid(&rec);
                  if !bad.is_empty() { status(&ui, &format!("形式が正しくありません: {}", bad.join("・"))); return; }
                  let old = std::mem::replace(&mut s.recs[i], rec);
                  if let Err(e) = persist(&s) { s.recs[i] = old; status(&ui, &e); return; }
              }
              2 => {
                  if let Some(o) = s.orig.clone() { ui.global::<Form>().invoke_put((&o).into()); }
                  return;
              }
              3 => {
                  let old = s.recs.remove(i);
                  if let Err(e) = persist(&s) { s.recs.insert(i, old); status(&ui, &e); return; }
              }
              _ => {}
          }
          s.editing = None; s.orig = None;
          ui.global::<Form>().invoke_clear();
          let bk = ui.global::<Backend>();
          bk.set_editing(false);
          bk.set_screen(1);
          status(&ui, "");
          refresh_list(&ui, &s);
      }); }

    // 並び替え・検索
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_sort(move |asc| {
          let ui = w.unwrap(); let mut s = a.borrow_mut();
          s.hist_asc = asc; s.list_asc = asc;
          s.cfg.sort_asc = asc; s.cfg.save();
          ui.global::<Backend>().set_sort_asc(asc);
          refresh_list(&ui, &s); refresh_hist(&ui, &s);
      }); }
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_search(move |q| { let ui = w.unwrap(); let mut s = a.borrow_mut(); s.query = q.to_string(); ui.global::<Backend>().set_query(q); refresh_list(&ui, &s); }); }

    // 起動時：最後に開いたファイルの一覧表示から。無ければ案内を表示
    {
        let mut s = app.borrow_mut();
        let last = s.cfg.last_file.clone().map(PathBuf::from).filter(|p| p.exists());
        let opened = match last { Some(p) => open_file(&ui, &mut s, p).is_ok(), None => false };
        if !opened {
            bk.set_message("「ファイル」メニューから、ファイルを開くか新規作成してください".into());
            bk.set_screen(1);
        }
    }

    ui.show()?;
    #[cfg(windows)]
    fit_to_screen(&ui);
    #[cfg(not(windows))]
    ui.window().set_maximized(true);
    ui.run()
}
