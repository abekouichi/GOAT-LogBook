#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use chrono::{Local, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    io::Read,
    rc::Rc,
    sync::{Arc, Mutex},
};

slint::include_modules!();

// ヘルプに表示する文章（アプリに内蔵）。取扱説明書を直したときは docs/manual.txt も直す
const FAQ_TEXT: &str = include_str!("../docs/faq.txt");
const MANUAL_TEXT: &str = include_str!("../docs/manual.txt");
const LICENSE_TEXT: &str = include_str!("../LICENSE");

const FREQS: [&str; 17] = ["1.9MHz", "3.5MHz", "3.8MHz", "7MHz", "10MHz", "14MHz", "18MHz", "21MHz", "24MHz", "28MHz",
                           "50MHz", "144MHz", "430MHz", "1200MHz", "2400MHz", "5600MHz", "10.1GHz"];
const MODES: [&str; 7] = ["CW", "SSB", "AM", "FM", "FT8", "RTTY", "SSTV"];
const POWERS: [&str; 7] = ["1W", "5W", "10W", "50W", "100W", "200W", "1kW"];
// {callsign} が呼出符号に置換されます。既存コードのURLがある場合は setting.json の api_url で上書きしてください。
const NCOL: usize = 19;
/// 一覧・PDFの項目名（番号＝項目番号。0〜14は従来どおり、15〜18が新しい項目）
const COL_HEADS: [&str; NCOL] = ["日付", "開始時刻", "終了時刻", "コールサイン", "RST(S)", "RST(R)", "周波数", "モード", "出力",
                                 "名前", "QTH", "常置場所", "市郡区ナンバー", "一括表示", "メモ", "GL", "検索GL", "距離", "方角"];
/// 既定の表示順（左から）
const DEF_ORDER: [usize; NCOL] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 15, 16, 17, 18, 14];
/// 既定の列幅[px]
const DEF_WIDTHS: [f32; NCOL] = [130.0, 70.0, 70.0, 130.0, 60.0, 60.0, 84.0, 62.0, 68.0, 124.0, 170.0, 170.0, 110.0, 170.0, 240.0, 90.0, 90.0, 110.0, 130.0];
/// 等幅フォントで表示する項目（日付・時刻・コールサイン・RST・周波数・モード・出力・市郡区ナンバー・一括表示・GL・検索GL・距離）
fn is_mono_col(c: usize) -> bool { matches!(c, 0..=8 | 12 | 13 | 15 | 16 | 17) }
const PREFS: [&str; 47] = ["北海道", "青森県", "岩手県", "秋田県", "山形県", "宮城県", "福島県", "新潟県", "長野県", "東京都", "神奈川県", "千葉県",
    "埼玉県", "茨城県", "栃木県", "群馬県", "山梨県", "静岡県", "岐阜県", "愛知県", "三重県", "京都府", "滋賀県", "奈良県", "大阪府", "和歌山県",
    "兵庫県", "富山県", "福井県", "石川県", "岡山県", "島根県", "山口県", "鳥取県", "広島県", "香川県", "徳島県", "愛媛県", "高知県", "福岡県",
    "佐賀県", "長崎県", "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県"];
const JARL_BASE: &str = "https://www.jarl.org/Japanese/A_Shiryo/A-2_jcc-jcg/";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) logbook/0.1";
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
    #[serde(rename = "GL", default)] gl: String,
    #[serde(rename = "検索GL", default)] ngl: String,
    #[serde(rename = "距離", default)] dist: String,
    #[serde(rename = "方角", default)] bear: String,
}

impl From<Entry> for Record {
    fn from(e: Entry) -> Self {
        Record {
            date: e.date.to_string(), start: e.t_start.to_string(), end: e.t_end.to_string(),
            call: e.call.to_string(), rst_s: e.rst_s.to_string(), rst_r: e.rst_r.to_string(),
            freq: e.freq.to_string(), mode: e.mode.to_string(), power: e.power.to_string(),
            name: e.name.to_string(), qth: e.qth.to_string(), memo: e.memo.to_string(),
            loc: e.loc.to_string(), num: e.num.to_string(), spec: e.spec.to_string(),
            gl: e.gl.to_string(), ngl: e.ngl.to_string(), dist: e.dist.to_string(), bear: e.bear.to_string(),
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
            gl: r.gl.as_str().into(), ngl: r.ngl.as_str().into(), dist: r.dist.as_str().into(), bear: r.bear.as_str().into(),
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
    #[serde(default)] my_gl: String,          // 自局のグリッドロケーター
    #[serde(default)] col_order: Vec<usize>,  // 一覧・履歴・PDFの表示順（左から）
    #[serde(default = "default_api")] api_url: String,
    #[serde(default)] last: Vec<String>, // 前回の入力：周波数・モード・出力・RST(S)・RST(R)
}
fn yes() -> bool { true }
fn default_api() -> String { DEFAULT_API.to_string() }
impl Default for Config {
    fn default() -> Self { Config { last_file: None, use_api: true, sort_asc: true, cols: vec![], cols_ver: 0, dark: false, show_clock: true, show_seconds: true, show_popup: true, margins: vec![], font_regular: None, font_bold: None, font_mono: None, my_call: String::new(), col_show: vec![], my_gl: String::new(), col_order: vec![], api_url: default_api(), last: vec![] } }
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
    colw: Rc<VecModel<f32>>, // 一覧・履歴の列幅（画面と共有）
    last: [String; 5], // 0=周波数 1=モード 2=出力 3=RST(S) 4=RST(R)
    editing: Option<usize>,
    orig: Option<Record>,
    hist_call: String,
    hist_asc: bool,
    list_asc: bool,
    query: String,
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
        gl: r.gl.as_str().into(), ngl: r.ngl.as_str().into(), dist: r.dist.as_str().into(), bear: r.bear.as_str().into(),
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
               &r.name, &r.qth, &r.memo, &r.loc, &r.num, &r.spec, &r.gl, &r.ngl, &r.dist, &r.bear]
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

/// 入力中の内容があるか（自動入力される日付・時刻・周波数・モード・出力は除く。RSTは、自動で入る開始値のままなら除く）
fn has_input(ui: &MainWindow, last: &[String; 5]) -> bool {
    let f = ui.global::<Form>();
    let (rs, rr) = (f.get_rst_s(), f.get_rst_r());
    let rst_typed = (!rs.is_empty() && rs.as_str() != rst_start(&last[3])) || (!rr.is_empty() && rr.as_str() != rst_start(&last[4]));
    rst_typed || [f.get_call(), f.get_name(), f.get_qth(), f.get_memo(), f.get_gl()].iter().any(|v| !v.is_empty())
}

/// 印刷の余白[mm]（上・下・左・右）。未設定なら10mm
fn margins_of(cfg: &Config) -> [i32; 4] {
    if cfg.margins.len() == 4 { [cfg.margins[0], cfg.margins[1], cfg.margins[2], cfg.margins[3]] } else { [10; 4] }
}

/// リスト・履歴の表示項目（19個）。未設定なら全部表示
/// 旧形式（15個）は、新しい4項目（GL・検索GL・距離・方角）を表示にして移行。
/// さらに古い形式（12個：メモが最後）は、QTHの後ろに常置場所・市郡区ナンバー・一括表示（表示）を挿入して移行
fn col_flags(cfg: &Config) -> Vec<bool> {
    match cfg.col_show.len() {
        19 => cfg.col_show.clone(),
        15 => { let mut v = cfg.col_show.clone(); v.extend([true; 4]); v }
        12 => {
            let mut v = cfg.col_show[..11].to_vec();
            v.extend([true; 3]);
            v.push(cfg.col_show[11]);
            v.extend([true; 4]);
            v
        }
        _ => vec![true; NCOL],
    }
}

/// 一覧・履歴の表示順（項目番号を左から並べたもの）。正しくなければ既定の順
fn col_order(cfg: &Config) -> Vec<usize> {
    let o = &cfg.col_order;
    if is_perm(o) { o.clone() } else { DEF_ORDER.to_vec() }
}

fn is_perm(o: &[usize]) -> bool {
    if o.len() != NCOL { return false; }
    let mut seen = [false; NCOL];
    for &c in o {
        if c >= NCOL || seen[c] { return false; }
        seen[c] = true;
    }
    true
}

/// 列幅。保存済みの値（旧形式の11個・14個も可）に、足りない分は既定値を足す
fn col_widths(cfg: &Config) -> Vec<f32> {
    let mut w = DEF_WIDTHS.to_vec();
    let c = &cfg.cols;
    if c.len() == 11 || c.len() == 14 || c.len() == NCOL {
        for (i, x) in c.iter().enumerate() { w[i] = x.max(30.0); }
    }
    w
}

/// 表示する項目と順序を画面に反映する
fn set_cols(ui: &MainWindow, order: &[usize], flags: &[bool]) {
    let bk = ui.global::<Backend>();
    bk.set_col_show(ModelRc::new(VecModel::from(flags.to_vec())));
    let vis: Vec<i32> = order.iter().filter(|&&c| flags[c]).map(|&c| c as i32).collect();
    bk.set_vis_cols(ModelRc::new(VecModel::from(vis)));
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
    r.gl = gl_norm(r.gl.trim());
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
    if kind == 5 { return filter_gl(text, cur); }
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
    if !r.gl.is_empty() && !gl_valid(&r.gl) { v.push("GL"); }
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

/// 日付(2)・時刻(3)の ← →：年・月・日／時・分の区切りへカーソルを動かす（区切りの末尾に置く）
fn seg_move(kind: i32, cur: i32, dir: i32) -> i32 {
    let ends: &[i32] = if kind == 2 { &[4, 7, 10] } else { &[2, 5] };
    if dir > 0 {
        ends.iter().copied().find(|&e| e > cur).unwrap_or(*ends.last().unwrap_or(&cur))
    } else {
        ends.iter().rev().copied().find(|&e| e < cur).unwrap_or(ends[0])
    }
}

fn options(kind: i32) -> Vec<String> {
    match kind {
        0 => FREQS.iter().map(|f| f.to_string()).collect(),
        1 => MODES.iter().map(|s| s.to_string()).collect(),
        _ => POWERS.iter().map(|s| s.to_string()).collect(),
    }
}

/// RSTの開始値：前回が3桁なら599、それ以外（2桁・前回なし）は59
fn rst_start(last: &str) -> String {
    if last.len() == 3 { "599".to_string() } else { "59".to_string() }
}

/// RSTの↑↓：2桁なら1の桁、3桁なら10の桁（S）を 1〜9 の範囲で増減する
/// RSTの↑↓：選んだ桁（idx＝左端が0）を増減する（最上位は1〜5、ほかは1〜9）
fn rst_step(text: &str, idx: i32, dir: i32) -> String {
    let n = text.len();
    if n != 2 && n != 3 { return rst_start(text); }
    let mut b = text.as_bytes().to_vec();
    let i = (idx.max(0) as usize).min(n - 1);
    if !b[i].is_ascii_digit() { return text.to_string(); }
    let max = if i == 0 { 5 } else { 9 }; // 最上位の桁（R）は5まで
    b[i] = (((b[i] - b'0') as i32 + dir).clamp(1, max) as u8) + b'0';
    String::from_utf8(b).unwrap_or_else(|_| text.to_string())
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

// ---------------------------------------------------------------- グリッドロケーター（GL）と距離・方角
/// GLの先頭部分として正しいか（1〜2文字目: A〜R、3〜4文字目: 数字、5〜6文字目: A〜X。最大6文字）
fn gl_prefix_ok(b: &[u8]) -> bool {
    if b.len() > 6 { return false; }
    b.iter().enumerate().all(|(i, &c)| match i {
        0 | 1 => (b'A'..=b'R').contains(&c.to_ascii_uppercase()),
        2 | 3 => c.is_ascii_digit(),
        _ => (b'A'..=b'X').contains(&c.to_ascii_uppercase()),
    })
}

/// 4桁（PM97）または6桁（PM97kk）の正しいGLか
fn gl_valid(s: &str) -> bool { (s.len() == 4 || s.len() == 6) && gl_prefix_ok(s.as_bytes()) }

/// 大文字小文字をそろえる（1〜2文字目は大文字、5〜6文字目は小文字）
fn gl_norm(s: &str) -> String {
    s.chars().enumerate().map(|(i, c)| if i < 2 { c.to_ascii_uppercase() } else if i >= 4 { c.to_ascii_lowercase() } else { c }).collect()
}

/// GL入力欄の入力制限。正しくない文字は受け付けない
fn filter_gl(text: &str, cur: i32) -> (String, i32) {
    let mut out = String::new();
    for c in text.chars() {
        if !c.is_ascii_alphanumeric() { continue; }
        let mut t = out.clone();
        t.push(c);
        if gl_prefix_ok(t.as_bytes()) { out = t; }
    }
    let out = gl_norm(&out);
    let removed = text.len().saturating_sub(out.len()) as i32;
    ((out.clone()), (cur - removed).clamp(0, out.len() as i32))
}

/// GLのマス目の番号（経度方向, 緯度方向）。prec=4 は 2°×1°、prec=6 は 5′×2.5′ のマス
fn gl_idx(gl: &str, prec: usize) -> Option<(i64, i64)> {
    let b: Vec<u8> = gl.bytes().map(|c| c.to_ascii_uppercase()).collect();
    if !(b.len() == 4 || b.len() == 6) || !gl_prefix_ok(&b) { return None; }
    let f = |i: usize| (b[i] - b'A') as i64;
    let d = |i: usize| (b[i] - b'0') as i64;
    let (lo, la) = (f(0) * 10 + d(2), f(1) * 10 + d(3));
    if prec == 6 {
        if b.len() < 6 { return None; }
        Some((lo * 24 + f(4), la * 24 + f(5)))
    } else {
        Some((lo, la))
    }
}

/// GLのマス目の中心（緯度, 経度）
fn gl_center(gl: &str) -> Option<(f64, f64)> {
    let b: Vec<u8> = gl.bytes().map(|c| c.to_ascii_uppercase()).collect();
    if !(b.len() == 4 || b.len() == 6) || !gl_prefix_ok(&b) { return None; }
    let mut lon = -180.0 + 20.0 * (b[0] - b'A') as f64 + 2.0 * (b[2] - b'0') as f64;
    let mut lat = -90.0 + 10.0 * (b[1] - b'A') as f64 + (b[3] - b'0') as f64;
    if b.len() == 6 {
        lon += (b[4] - b'A') as f64 * (5.0 / 60.0) + 2.5 / 60.0;
        lat += (b[5] - b'A') as f64 * (2.5 / 60.0) + 1.25 / 60.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Some((lat, lon))
}

/// 緯度・経度から6桁のGLを求める
fn gl_from_latlon(lat: f64, lon: f64) -> String {
    let lo = (lon + 180.0).clamp(0.0, 359.999_999);
    let la = (lat + 90.0).clamp(0.0, 179.999_999);
    let f1 = (lo / 20.0).floor();
    let r = lo - f1 * 20.0;
    let s1 = (r / 2.0).floor();
    let u1 = ((r - s1 * 2.0) * 12.0).floor().min(23.0);
    let f2 = (la / 10.0).floor();
    let r = la - f2 * 10.0;
    let s2 = r.floor();
    let u2 = ((r - s2) * 24.0).floor().min(23.0);
    format!("{}{}{}{}{}{}", (b'A' + f1 as u8) as char, (b'A' + f2 as u8) as char, s1 as u8, s2 as u8,
            (b'a' + u1 as u8) as char, (b'a' + u2 as u8) as char)
}

/// 2点間の距離[km]と、1点目から見た2点目の方位[度]（大圏）
fn dist_bear(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (p1, p2) = (a.0.to_radians(), b.0.to_radians());
    let dl = (b.1 - a.1).to_radians();
    let h = ((p2 - p1) / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    let d = 2.0 * 6371.0 * h.sqrt().min(1.0).asin();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (d, y.atan2(x).to_degrees().rem_euclid(360.0))
}

fn compass16(deg: f64) -> &'static str {
    const N: [&str; 16] = ["北", "北北東", "北東", "東北東", "東", "東南東", "南東", "南南東",
                           "南", "南南西", "南西", "西南西", "西", "西北西", "北西", "北北西"];
    N[(((deg + 11.25) / 22.5).floor() as usize) % 16]
}

fn calc_dist_opt(my: &str, gl: &str, ngl: &str) -> Option<(String, String)> {
    let my = my.trim();
    if !gl_valid(my) { return None; }
    let gl = gl.trim();
    let ngl = ngl.trim();
    // 入力したGLを優先。なければ、ネットで調べた相手局のGL（かっこつきは、移動運用などで不確か）
    let (other, uncertain) = if gl_valid(gl) {
        (gl.to_string(), false)
    } else {
        let paren = ngl.starts_with('(');
        let n = ngl.trim_matches(|c: char| c == '(' || c == ')');
        if gl_valid(n) { (n.to_string(), paren) } else { return None; }
    };
    // 比べるマスの大きさ：両方6桁なら6桁、どちらかが4桁なら4桁
    let prec = if my.len() >= 6 && other.len() >= 6 { 6 } else { 4 };
    let (a, b) = (gl_idx(my, prec)?, gl_idx(&other, prec)?);
    let n: i64 = if prec == 6 { 4320 } else { 180 };
    let dx = (a.0 - b.0).abs();
    let ring = dx.min(n - dx).max((a.1 - b.1).abs()); // 0=同じマス 1=隣接（8マス） 2=その外側（16マス）
    if ring == 0 { return Some(("-".into(), "-".into())); }
    let (d, br) = dist_bear(gl_center(my)?, gl_center(&other)?);
    let ds = format!("{} km", d.round() as i64);
    let bs = format!("{} {}°", compass16(br), (br.round() as i64) % 360);
    if ring <= 2 || uncertain { Some((format!("({ds})"), format!("({bs})"))) } else { Some((ds, bs)) }
}

/// 自局のGL・入力したGL・ネットで調べたGLから、(距離, 方角) の表示を作る
fn calc_dist(my: &str, gl: &str, ngl: &str) -> (String, String) {
    calc_dist_opt(my, gl, ngl).unwrap_or_default()
}

/// 入力画面の距離・方角を、いまの入力値から計算し直す
fn refresh_dist(ui: &MainWindow) {
    let (bk, f) = (ui.global::<Backend>(), ui.global::<Form>());
    let (d, b) = calc_dist(bk.get_my_gl().as_str(), f.get_gl().as_str(), f.get_ngl().as_str());
    f.set_dist(d.into());
    f.set_bear(b.into());
}

// ---------------------------------------------------------------- JARL 市郡区ナンバー（ネットから取得）
/// JARLのJCC（市）・JCG（郡）・区番号のリスト。ファイルは使わず、最初の検索のときにネットから読み込む
#[derive(Default)]
struct JccDb {
    cities: Vec<(usize, String, String, bool)>, // (都道府県番号, 市名, 番号, 東京都の特別区か)
    kus: HashMap<(String, String), String>,     // (市の番号, 区名) → 区番号
    guns: Vec<(usize, String, String, bool)>,   // (都道府県番号, 郡名, 番号, 括弧つき（北海道で同名の郡がある）か)
    towns: HashMap<(usize, String), String>,    // (都道府県番号, 町村名) → 郡番号（リストに町村が載っている郡）
}

fn nk(c: char) -> char { match c { 'ヶ' | 'ヵ' => 'ケ', _ => c } }
fn nstr(s: &str) -> String { s.chars().filter(|c| !c.is_whitespace()).map(nk).collect() }
fn all_digits(s: &str) -> bool { !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) }

/// JCCリスト：「  0802   Nagaoka   長岡」の形。「*」つきは消滅した市なので除く
fn parse_jcc(text: &str, db: &mut JccDb) {
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('*') || t.contains('※') { continue; }
        let tok: Vec<&str> = t.split_whitespace().collect();
        if tok.len() < 3 || !all_digits(tok[0]) || !(tok[0].len() == 4 || tok[0].len() == 6) { continue; }
        let Ok(pref) = tok[0][..2].parse::<usize>() else { continue };
        db.cities.push((pref, nstr(tok[2]), tok[0].to_string(), tok[0].len() == 6));
    }
}

/// JCGリスト：「  08001   Iwafune   岩船」の形。北海道などは、続く行に町村名が載っている
fn parse_jcg(text: &str, db: &mut JccDb) {
    let mut cur: Option<(usize, String)> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() { continue; }
        if t.starts_with('*') { cur = None; continue; }
        let tok: Vec<&str> = t.split_whitespace().collect();
        if all_digits(tok[0]) && tok[0].len() == 5 && tok.len() >= 3 {
            let Ok(pref) = tok[0][..2].parse::<usize>() else { continue };
            let kanji = tok[2];
            let paren = kanji.contains('(') || kanji.contains('（');
            let name = nstr(kanji.split(|c: char| c == '(' || c == '（').next().unwrap_or(""));
            db.guns.push((pref, name, tok[0].to_string(), paren));
            cur = Some((pref, tok[0].to_string()));
        } else if let Some((pref, no)) = &cur {
            if tok.len() >= 2 {
                let last = tok[tok.len() - 1];
                if !last.is_ascii() { db.towns.insert((*pref, nstr(last)), no.clone()); }
            }
        }
    }
}

/// 区リスト：「札幌市(0101)」の行のあとに「010101  Chuo  中央」が続く。「※」つきは消滅した区
fn parse_ku(text: &str, db: &mut JccDb) {
    let mut city: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() { continue; }
        if let Some(p) = t.find(|c: char| c == '(' || c == '（') {
            if t[..p].ends_with('市') {
                let d: String = t[p..].chars().filter(|c| c.is_ascii_digit()).collect();
                city = if d.is_empty() { None } else { Some(d) };
                continue;
            }
        }
        if t.contains('※') { continue; }
        let tok: Vec<&str> = t.split_whitespace().collect();
        if let Some(c) = &city {
            if tok.len() >= 3 && all_digits(tok[0]) && tok[0].len() == 6 {
                db.kus.insert((c.clone(), nstr(tok[2])), tok[0].to_string());
            }
        }
    }
}

fn decode_text(b: &[u8]) -> String {
    let s = match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(b).0.into_owned(), // JARLのファイルがShift_JISのときのため
    };
    s.trim_start_matches('\u{feff}').to_string()
}

fn http_text(url: &str) -> Result<String, String> {
    let resp = ureq::get(url).set("User-Agent", UA).timeout(std::time::Duration::from_secs(15)).call()
        .map_err(|e| match e {
            ureq::Error::Status(c, _) => format!("HTTP {c}: {url}"),
            other => format!("通信エラー: {other}"),
        })?;
    let mut buf = Vec::new();
    resp.into_reader().take(8_000_000).read_to_end(&mut buf).map_err(|e| format!("取得エラー: {e}"))?;
    Ok(decode_text(&buf))
}

fn load_jcc_db() -> Result<JccDb, String> {
    let mut db = JccDb::default();
    parse_jcc(&http_text(&format!("{JARL_BASE}jcc-list.txt")).map_err(|e| format!("JARL市番号リスト: {e}"))?, &mut db);
    parse_jcg(&http_text(&format!("{JARL_BASE}jcg-list.txt")).map_err(|e| format!("JARL郡番号リスト: {e}"))?, &mut db);
    parse_ku(&http_text(&format!("{JARL_BASE}ku-list.txt")).map_err(|e| format!("JARL区番号リスト: {e}"))?, &mut db);
    if db.cities.is_empty() { return Err("JARL市番号リストを読み取れませんでした".into()); }
    Ok(db)
}

static JCC_DB: Mutex<Option<Arc<JccDb>>> = Mutex::new(None);

/// JARLのリストを、最初に必要になったときに1回だけ読み込む（失敗したら次の検索でもう一度）
fn jcc_db() -> Result<Arc<JccDb>, String> {
    let mut g = JCC_DB.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(d) = g.as_ref() { return Ok(d.clone()); }
    let d = Arc::new(load_jcc_db()?);
    *g = Some(d.clone());
    Ok(d)
}

/// 常置場所（例「新潟県長岡市」「新潟県新潟市中央区」「新潟県北蒲原郡聖籠町」）から、
/// (市郡区ナンバー, 市区町村までの住所) を求める。政令指定都市は区ナンバー
fn resolve(db: &JccDb, loc: &str) -> Option<(String, String)> {
    let orig: Vec<char> = loc.trim().chars().collect();
    let norm: String = orig.iter().map(|&c| nk(c)).collect();
    let (pi, pname) = PREFS.iter().enumerate().find(|(_, p)| norm.starts_with(**p))?;
    let pref = pi + 1;
    let plen = pname.chars().count();
    let rest: String = norm.chars().skip(plen).collect();
    let rc: Vec<char> = rest.chars().collect();
    let mk = |n: usize| -> String { orig.iter().take(plen + n).collect() };
    let nlen = |s: &str| s.chars().count();

    // 市（名前がいちばん長く合うもの）
    let mut best: Option<&(usize, String, String, bool)> = None;
    for c in db.cities.iter().filter(|c| c.0 == pref && !c.3) {
        if rest.starts_with(&format!("{}市", c.1)) && best.map_or(true, |b| nlen(&c.1) > nlen(&b.1)) { best = Some(c); }
    }
    if let Some(c) = best {
        let n = nlen(&c.1) + 1;
        let tail: Vec<char> = rc.iter().skip(n).copied().collect();
        if let Some(k) = tail.iter().take(6).position(|&ch| ch == '区') {
            let ku: String = tail[..k].iter().collect();
            if let Some(num) = db.kus.get(&(c.2.clone(), ku)) { return Some((num.clone(), mk(n + k + 1))); }
        }
        return Some((c.2.clone(), mk(n)));
    }
    // 東京都の特別区
    if pref == 10 {
        let mut w: Option<&(usize, String, String, bool)> = None;
        for c in db.cities.iter().filter(|c| c.0 == pref && c.3) {
            if rest.starts_with(&format!("{}区", c.1)) && w.map_or(true, |b| nlen(&c.1) > nlen(&b.1)) { w = Some(c); }
        }
        if let Some(c) = w { return Some((c.2.clone(), mk(nlen(&c.1) + 1))); }
    }
    // 郡＋町村
    if let Some(g) = rc.iter().position(|&ch| ch == '郡') {
        if g >= 1 {
            let gun: String = rc[..g].iter().collect();
            let after = &rc[g + 1..];
            let t = after.iter().position(|&ch| ch == '町' || ch == '村');
            let n = t.map(|t| g + 1 + t + 1).unwrap_or(g + 1);
            if let Some(t) = t {
                let town: String = after[..=t].iter().collect();
                if let Some(num) = db.towns.get(&(pref, town)) { return Some((num.clone(), mk(n))); }
            }
            if let Some(x) = db.guns.iter().find(|x| x.0 == pref && x.1 == gun && !x.3) { return Some((x.2.clone(), mk(n))); }
        }
    }
    // 郡のない町村（東京都の島など）
    if let Some(t) = rc.iter().position(|&ch| ch == '町' || ch == '村') {
        let town: String = rc[..=t].iter().collect();
        if let Some(num) = db.towns.get(&(pref, town)) { return Some((num.clone(), mk(t + 1))); }
    }
    None
}

/// 国土地理院の住所検索で、市区町村の位置を調べ、6桁のGLにする
fn geocode_gl(q: &str) -> Result<String, String> {
    let body = ureq::get("https://msearch.gsi.go.jp/address-search/AddressSearch")
        .query("q", q).set("User-Agent", UA).timeout(std::time::Duration::from_secs(10)).call()
        .map_err(|e| match e {
            ureq::Error::Status(c, _) => format!("HTTP {c}"),
            other => format!("通信エラー: {other}"),
        })?
        .into_string().map_err(|e| format!("取得エラー: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("解析エラー: {e}"))?;
    let arr = v.as_array().ok_or("住所が見つかりません")?;
    let pick = arr.iter().find(|f| f["properties"]["title"].as_str() == Some(q)).or_else(|| arr.first()).ok_or("住所が見つかりません")?;
    let c = pick["geometry"]["coordinates"].as_array().ok_or("位置を読み取れません")?;
    let (lon, lat) = (c.get(0).and_then(|x| x.as_f64()), c.get(1).and_then(|x| x.as_f64()));
    match (lat, lon) {
        (Some(la), Some(lo)) => Ok(gl_from_latlon(la, lo)),
        _ => Err("位置を読み取れません".into()),
    }
}

/// 総務省APIと、JARL・国土地理院から取得した結果
struct Found { loc: String, num: String, spec: String, gl: String, warn: Option<String> }

fn fetch(tpl: &str, call: &str, paren: bool) -> Result<Found, String> {
    let mut url = tpl.replace("{callsign}", &base(call));
    if !url.contains("SC=") { url.push_str("&SC=1"); } // スタートカウント必須（保存済みの旧URLにも対応）
    let body = ureq::get(&url)
        .set("User-Agent", UA)
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
        return Ok(Found { loc: nf.clone(), num: nf.clone(), spec: nf, gl: String::new(), warn: None });
    }
    // 市郡区ナンバー：JARLのリストをネットから読み込んで調べる
    let mut warns: Vec<String> = vec![];
    let db = jcc_db();
    let mut nums: Vec<String> = vec![];
    let mut munis: Vec<String> = vec![];
    match &db {
        Ok(d) => for l in &locs {
            match resolve(d, l) {
                Some((n, m)) => { nums.push(n); munis.push(m); }
                None => nums.push("Not found".into()),
            }
        },
        Err(e) => { nums.push("取得エラー".into()); warns.push(e.clone()); }
    }
    let nums = dedup(nums);
    // 相手局のGL：市区町村の位置を、国土地理院の住所検索でネットから調べる
    let mut gl = String::new();
    let q = munis.first().cloned().or_else(|| locs.first().cloned()).unwrap_or_default();
    if !q.is_empty() {
        match geocode_gl(&q) {
            Ok(g) => gl = if paren { format!("({g})") } else { g },
            Err(e) => warns.push(format!("GL取得エラー: {e}")),
        }
    }
    let wrap = |s: String| if paren { format!("({s})") } else { s };
    Ok(Found { loc: wrap(locs.join(",")), num: wrap(nums.join(",")), spec: specs.join(","), gl,
               warn: if warns.is_empty() { None } else { Some(warns.join(" / ")) } })
}

fn start_lookup(ui: &MainWindow, tpl: String, call: String) {
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let res = fetch(&tpl, &call, call.contains('/'));
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let f = ui.global::<Form>();
                if f.get_call().as_str() != call { return; } // 入力が変わっていたら捨てる
                match res {
                    Ok(r) => {
                        f.invoke_set_info(r.loc.into(), r.num.into(), r.spec.into(), r.gl.into());
                        refresh_dist(&ui);
                        if let Some(w) = r.warn { status(&ui, &w); }
                    }
                    Err(e) => {
                        f.invoke_set_info("取得エラー".into(), "".into(), "".into(), "".into());
                        refresh_dist(&ui);
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
    let heads = COL_HEADS;
    let cols: Vec<usize> = col_order(&s.cfg).into_iter().filter(|&c| flags[c]).collect(); // 設定した表示順（左から）
    if cols.is_empty() { return Err("表示する項目がありません。設定で表示項目を選んでください".into()); }
    let cell = |r: &Record, c: usize| -> String {
        match c {
            0 => r.date.clone(), 1 => r.start.clone(), 2 => r.end.clone(), 3 => r.call.clone(),
            4 => r.rst_s.clone(), 5 => r.rst_r.clone(), 6 => r.freq.clone(), 7 => r.mode.clone(),
            8 => r.power.clone(), 9 => r.name.clone(), 10 => r.qth.clone(), 11 => r.loc.clone(),
            12 => r.num.clone(), 13 => r.spec.clone(), 14 => r.memo.clone(),
            15 => r.gl.clone(), 16 => r.ngl.clone(), 17 => r.dist.clone(), _ => r.bear.clone(),
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
    let mono: Vec<bool> = cols.iter().map(|&c| is_mono_col(c)).collect(); // 日付・時刻・コールサイン・RST・周波数・モード・出力・市郡区ナンバー・一括表示・GL・検索GL・距離は等幅（Windows: Consolas / Linux: Inconsolata、無ければ DejaVu Sans Mono など）
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
#[derive(Clone)]
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

/// TTC（複数の書体が入ったファイル）から、1つの書体を取り出して、単独のTTFにする。TTFならそのまま返す
fn extract_face(data: &[u8], index: u32) -> Option<Vec<u8>> {
    if data.get(0..4)? != b"ttcf" { return Some(data.to_vec()); }
    let be32 = |o: usize| -> Option<usize> { Some(u32::from_be_bytes(data.get(o..o + 4)?.try_into().ok()?) as usize) };
    let be16 = |o: usize| -> Option<usize> { Some(u16::from_be_bytes(data.get(o..o + 2)?.try_into().ok()?) as usize) };
    if index as usize >= be32(8)? { return None; }
    let off = be32(12 + 4 * index as usize)?;
    let num = be16(off + 4)?;
    let dir_len = 12 + 16 * num;
    let mut recs = vec![];
    for i in 0..num {
        let r = off + 12 + 16 * i;
        recs.push((data.get(r..r + 4)?.to_vec(), data.get(r + 4..r + 8)?.to_vec(), be32(r + 8)?, be32(r + 12)?));
    }
    let mut body: Vec<u8> = vec![];
    let mut offs = vec![];
    for (_, _, o, l) in &recs {
        offs.push(dir_len + body.len());
        body.extend_from_slice(data.get(*o..*o + *l)?);
        while body.len() % 4 != 0 { body.push(0); }
    }
    let mut out = data.get(off..off + 12)?.to_vec();
    for (i, (tag, cs, _, l)) in recs.iter().enumerate() {
        out.extend_from_slice(tag);
        out.extend_from_slice(cs);
        out.extend_from_slice(&(offs[i] as u32).to_be_bytes());
        out.extend_from_slice(&(*l as u32).to_be_bytes());
    }
    out.extend_from_slice(&body);
    Some(out)
}

/// フォントファイルを読む。TTCのときは、family_hint を名前に含む書体を選んで、単独のTTFにする
fn read_font(path: &Path, family_hint: Option<&str>) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    let mut index = 0u32;
    if let Some(h) = family_hint {
        let n = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        for i in 0..n {
            if let Ok(face) = ttf_parser::Face::parse(&data, i) {
                let hit = face.names().into_iter()
                    .any(|nm| (nm.name_id == 1 || nm.name_id == 16) && nm.to_string().map_or(false, |t| t.contains(h)));
                if hit { index = i; break; }
            }
        }
    }
    extract_face(&data, index)
}

const REG_NAMES: &[&str] = &["NotoSansJP-Regular.ttf", "NotoSansJP-VariableFont_wght.ttf", "NotoSansJP[wght].ttf", "NotoSansJP.ttf",
                             "NotoSansJP-Regular.otf", "NotoSansCJKjp-Regular.otf", "NotoSansCJK-Regular.ttc"];
const BOLD_NAMES: &[&str] = &["NotoSansJP-Bold.ttf", "NotoSansJP-Bold.otf", "NotoSansCJKjp-Bold.otf", "NotoSansCJK-Bold.ttc"];
/// 等幅フォントのファイル名（先頭ほど優先）。Windows は Consolas、Linux は Inconsolata を優先し、
/// 無ければ Linux に多い等幅フォント（DejaVu Sans Mono など）を使う。画面とPDFで同じフォントを使う
fn mono_names() -> &'static [&'static str] {
    if cfg!(windows) { &["consola.ttf"] }
    else {
        &["Inconsolata-Regular.ttf", "Inconsolata[wdth,wght].ttf", "Inconsolata-VariableFont_wdth,wght.ttf", "Inconsolata.ttf",
          "DejaVuSansMono.ttf", "LiberationMono-Regular.ttf",
          "NotoSansMono-Regular.ttf", "NotoSansMono[wdth,wght].ttf", "NotoSansMono-VariableFont_wdth,wght.ttf",
          "UbuntuMono-R.ttf", "UbuntuMono-Regular.ttf", "Cousine-Regular.ttf",
          "Inconsolata-Regular.otf", "Inconsolata.otf"]
    }
}

/// フォントを探すフォルダ（先頭ほど優先）。Windows・Linux の両方の標準的な場所を含む
fn font_dirs() -> Vec<PathBuf> {
    let mut v = vec![exe_dir().join("fonts")];
    if let Ok(w) = std::env::var("WINDIR") { v.push(PathBuf::from(w).join("Fonts")); }
    if let Ok(l) = std::env::var("LOCALAPPDATA") { v.push(PathBuf::from(l).join("Microsoft").join("Windows").join("Fonts")); }
    if let Ok(h) = std::env::var("HOME") {
        let h = PathBuf::from(h);
        v.push(h.join(".local/share/fonts"));
        v.push(h.join(".fonts"));
    }
    v.push(PathBuf::from("/usr/local/share/fonts"));
    v.push(PathBuf::from("/usr/share/fonts"));
    v
}

/// フォルダの中（サブフォルダも）のファイルを、（小文字のファイル名, パス）で集める
fn list_files(dir: &Path, depth: u32, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 { list_files(&p, depth - 1, out); }
        } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            out.push((n.to_lowercase(), p));
        }
    }
}

fn is_font_file(f: &str) -> bool { f.ends_with(".ttf") || f.ends_with(".otf") || f.ends_with(".ttc") }

/// 名前が少し違う等幅フォントのファイルも拾う（太字・斜体などは除く）
fn ok_mono_file(f: &str) -> bool {
    if cfg!(windows) { return f == "consola.ttf"; }
    let skip = ["bold", "black", "thin", "light", "medium", "extra", "demi", "semi", "italic", "oblique"];
    ["inconsolata", "dejavusansmono", "liberationmono", "notosansmono", "ubuntumono", "cousine"].iter().any(|p| f.starts_with(p))
        && is_font_file(f) && !skip.iter().any(|w| f.contains(w))
}

/// 等幅フォントのデータを探す（setting.json の font_mono → 各フォルダの順）。見つからなければ None
fn find_mono_data(cfg: &Config) -> Option<Vec<u8>> {
    if let Some(p) = &cfg.font_mono {
        if let Some(d) = read_font(Path::new(p), None) { return Some(d); }
    }
    for dir in font_dirs() {
        let mut files = vec![];
        list_files(&dir, 6, &mut files);
        for n in mono_names() {
            let n = n.to_lowercase();
            if let Some((_, p)) = files.iter().find(|(f, _)| *f == n) {
                if let Some(d) = read_font(p, None) { return Some(d); }
            }
        }
        for (_, p) in files.iter().filter(|(f, _)| ok_mono_file(f)) {
            if let Some(d) = read_font(p, None) { return Some(d); }
        }
    }
    None
}

/// フォントデータから、ファミリー名（画面で font-family に指定する名前）を読む
fn family_name(data: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    for nm in face.names() {
        if nm.name_id == 1 && nm.is_unicode() {
            if let Some(t) = nm.to_string() { return Some(t); }
        }
    }
    None
}

/// 画面で使う等幅フォントの名前。PDFと同じフォントファイルから名前を読むので、画面とPDFがそろう
fn mono_family(cfg: &Config) -> String {
    find_mono_data(cfg).and_then(|d| family_name(&d))
        .unwrap_or_else(|| if cfg!(windows) { "Consolas".into() } else { "DejaVu Sans Mono".into() })
}

/// PDFに使うフォントを読み込む。次の順に探す
///   1. setting.json で指定したファイル（font_regular / font_bold / font_mono）
///   2. 実行ファイルと同じ場所の fonts フォルダ
///   3. システムのフォントフォルダ（Windows: Windows\Fonts とユーザーのフォント、Linux: /usr/share/fonts など）
/// 本文・見出しは Noto Sans JP（無ければ Noto Sans CJK JP）、コールサインは Windows が Consolas、Linux が Inconsolata
fn load_fonts(cfg: &Config) -> Result<Fonts, String> {
    let dirs = font_dirs();
    let index: Vec<Vec<(String, PathBuf)>> = dirs.iter().map(|d| {
        let mut v = vec![];
        list_files(d, 6, &mut v);
        v
    }).collect();
    let pick = |custom: &Option<String>, names: &[&str], hint: Option<&str>, ok: &dyn Fn(&str) -> bool| -> Option<Vec<u8>> {
        if let Some(p) = custom {
            if let Some(d) = read_font(Path::new(p), hint) { return Some(d); }
        }
        for files in &index {
            for n in names {
                let n = n.to_lowercase();
                if let Some((_, p)) = files.iter().find(|(f, _)| *f == n) {
                    if let Some(d) = read_font(p, hint) { return Some(d); }
                }
            }
            // 名前が少し違うファイルも拾う
            for (_, p) in files.iter().filter(|(f, _)| ok(f)) {
                if let Some(d) = read_font(p, hint) { return Some(d); }
            }
        }
        None
    };
    let is_font = |f: &str| f.ends_with(".ttf") || f.ends_with(".otf") || f.ends_with(".ttc");
    let noto = |f: &str| (f.starts_with("notosansjp") || f.starts_with("notosanscjk")) && is_font(f) && !f.contains("mono") && !f.contains("serif");
    let weights = ["bold", "black", "thin", "light", "medium", "extra", "demi", "semi", "italic", "oblique"];
    let ok_regular = |f: &str| noto(f) && !weights.iter().any(|w| f.contains(w));
    let ok_bold = |f: &str| noto(f) && f.contains("bold") && !f.contains("extra") && !f.contains("semi");
    let d_regular = pick(&cfg.font_regular, REG_NAMES, Some("CJK JP"), &ok_regular)
        .ok_or_else(|| {
            let seen: Vec<String> = dirs.iter().zip(&index).map(|(d, f)| format!("{}（{}件）", d.display(), f.len())).collect();
            format!("Noto Sans JP が見つかりません。{} に NotoSansJP-Regular.ttf を置くか、setting.json の font_regular で指定してください。探した場所: {}",
                    dirs[0].display(), seen.join(" / "))
        })?;
    let d_bold = pick(&cfg.font_bold, BOLD_NAMES, Some("CJK JP"), &ok_bold).unwrap_or_else(|| d_regular.clone());
    let d_mono = find_mono_data(cfg).unwrap_or_else(|| d_regular.clone());
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
        if w + a > max_mm + 0.05 { break; } // 浮動小数の誤差で最後の1文字が欠けないよう、少し余裕を持たせる
        w += a;
        out.push(c);
    }
    out
}

/// 余白（上・下・左・右 [mm]）に従って、全ページの描画命令を作る
/// セルを等幅フォントで描くか。等幅フォントに無い文字（日本語など）を含むときは、本文フォントにする
fn use_mono(job: &PrintJob, fonts: &Fonts, c: usize, text: &str) -> bool {
    job.mono[c] && text.chars().all(|ch| fonts.mono.lookup_glyph_index(ch) != 0)
}

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

    // 列幅：内容の最大幅（上限70mm）。ただし見出しの幅より狭くはしない。
    // 用紙に収まらないときは、見出しより広い分だけを縮める。見出しだけでも収まらないときは、見出しの文字を小さくする。
    let n = job.heads.len();
    let extra = 2.0 * pad + 0.4; // 左右の余白 + 誤差の余裕
    let avail = (PAGE_W - ml - mr).max(30.0);
    let hw0: Vec<f32> = job.heads.iter().map(|h| text_w(&fonts.bold, h, body)).collect();
    let hsum: f32 = hw0.iter().sum();
    let hpt = if hsum + n as f32 * extra > avail { body * ((avail - n as f32 * extra) / hsum).clamp(0.3, 1.0) } else { body };
    let minw: Vec<f32> = job.heads.iter().map(|h| text_w(&fonts.bold, h, hpt) + extra).collect();
    let want: Vec<f32> = (0..n).map(|c| {
        let mut w = 0.0_f32;
        for r in &job.rows {
            let f = if use_mono(job, fonts, c, &r[c]) { &fonts.mono } else { &fonts.regular };
            w = w.max(text_w(f, &r[c], body));
            if w >= 70.0 { break; }
        }
        (w.min(70.0) + extra).max(minw[c])
    }).collect();
    let cw: Vec<f32> = if want.iter().sum::<f32>() > avail {
        let spare = (avail - minw.iter().sum::<f32>()).max(0.0);
        let grow: f32 = (0..n).map(|c| want[c] - minw[c]).sum();
        (0..n).map(|c| minw[c] + if grow > 0.0 { (want[c] - minw[c]) * spare / grow } else { 0.0 }).collect()
    } else { want };

    let body_h = PAGE_H - mt - mb - title_h - info_h - foot_h - rh;
    let rpp = ((body_h / rh).floor() as usize).max(1);
    let pages = ((job.rows.len() + rpp - 1) / rpp).max(1);

    let push_row = |cmds: &mut Vec<Cmd>, y: f32, cells: &[String], header: bool| {
        let mut x = ml;
        for (c, v) in cells.iter().enumerate() {
            let w = cw[c];
            cmds.push(Cmd::Rect { x, y, w, h: rh, fill: if header { Some(0.91) } else { None }, stroke: true });
            let (fk, f) = if header { (FontKind::Bold, &fonts.bold) }
                          else if use_mono(job, fonts, c, v) { (FontKind::Mono, &fonts.mono) }
                          else { (FontKind::Regular, &fonts.regular) };
            let size = if header { hpt } else { body };
            let t = fit_text(f, v, size, (w - 2.0 * pad).max(0.0));
            cmds.push(Cmd::Text { x: x + pad, y: y + vpad + asc(body), size, font: fk, text: t });
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
fn write_pdf(title: &str, pages: &[Vec<Cmd>], fonts: &Fonts, path: &Path) -> Result<(), String> {
    use printpdf::path::PaintMode;
    use printpdf::*;
    use std::io::{BufWriter, Cursor};
    let (pdf, p0, l0) = PdfDocument::new(title, Mm(PAGE_W as _), Mm(PAGE_H as _), "layer");
    let load = |k: FontKind| pdf.add_external_font(Cursor::new(fonts.data(k).clone())).map_err(|e| format!("フォントをPDFに埋め込めません: {e}"));
    let f_reg = load(FontKind::Regular)?;
    let f_bold = if fonts.d_bold == fonts.d_regular { f_reg.clone() } else { load(FontKind::Bold)? };
    let f_mono = if fonts.d_mono == fonts.d_regular { f_reg.clone() } else { load(FontKind::Mono)? };
    for (i, cmds) in pages.iter().enumerate() {
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
    ui.global::<Metrics>().set_mono_font(mono_family(&cfg).into());
    // 一時的に1.5倍にして保存した列幅（cols_ver == 1）を元の大きさに戻す
    if cfg.cols_ver < 2 {
        if cfg.cols_ver == 1 && !cfg.cols.is_empty() {
            cfg.cols = cfg.cols.iter().map(|x| x / 1.5).collect();
        }
        cfg.cols_ver = 2;
        cfg.save();
    }
    // 等幅フォントの文字を1.1倍にしたので、等幅の項目の列幅も1.1倍にする（1回だけ）
    if cfg.cols_ver < 3 {
        for (i, x) in cfg.cols.iter_mut().enumerate() {
            if is_mono_col(i) { *x *= 1.1; }
        }
        cfg.cols_ver = 3;
        cfg.save();
    }
    let colw = Rc::new(VecModel::from(col_widths(&cfg)));
    let asc = cfg.sort_asc;
    let mut last: [String; 5] = Default::default();
    for (i, v) in cfg.last.iter().take(5).enumerate() { last[i] = v.clone(); }
    let app: Shared = Rc::new(RefCell::new(App {
        cfg, file: None, recs: vec![], colw: colw.clone(), last, editing: None, orig: None,
        hist_call: String::new(), hist_asc: asc, list_asc: asc, query: String::new(), doc: None, fonts: None,
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
    bk.set_my_gl(app.borrow().cfg.my_gl.as_str().into());
    bk.set_col_w(ModelRc::from(colw.clone()));
    {
        let s = app.borrow();
        set_cols(&ui, &col_order(&s.cfg), &col_flags(&s.cfg));
    }

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
      bk.on_last(move |k| {
          let k = (k as usize).min(4);
          a.try_borrow().map(|s| if k >= 3 { rst_start(&s.last[k]) } else { s.last[k].clone() }).unwrap_or_default().into()
      }); }
    bk.on_rst_step(|t, i, d| rst_step(&t, i, d).into());
    bk.on_seg(|k, c, d| seg_move(k, c, d));
    bk.on_calc(|my, gl, ngl| { let (d, b) = calc_dist(&my, &gl, &ngl); Dist { dist: d.into(), bear: b.into() } });
    { let a = app.clone();
      bk.on_resize(move |c, w| {
          if let Ok(s) = a.try_borrow() {
              let c = c as usize;
              if c < NCOL { s.colw.set_row_data(c, w.max(30.0)); }
          }
      }); }

    // 画面遷移
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_goto(move |n| {
          let ui = w.unwrap(); let mut s = a.borrow_mut(); let bk = ui.global::<Backend>();
          if s.editing.is_some() {
              status(&ui, "編集中は他のモードに移れません。「保存して終了」「元に戻す」「保存せず終了」「削除」のいずれかで終了してください");
              return;
          }
          if n == 1 {
              if has_input(&ui, &s.last) {
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
          let ui = w.unwrap(); let s = a.borrow(); let bk = ui.global::<Backend>();
          bk.set_d_use_api(s.cfg.use_api);
          bk.set_d_dark(s.cfg.dark);
          bk.set_d_clock(s.cfg.show_clock);
          bk.set_d_sec(s.cfg.show_seconds);
          bk.set_d_popup(s.cfg.show_popup);
          bk.set_d_call(s.cfg.my_call.as_str().into());
          bk.set_d_gl(s.cfg.my_gl.as_str().into());
          bk.set_d_col_show(ModelRc::new(VecModel::from(col_flags(&s.cfg))));
          let order: Vec<i32> = col_order(&s.cfg).into_iter().map(|c| c as i32).collect();
          bk.set_d_col_order(ModelRc::new(VecModel::from(order)));
      }); }

    // 設定メニュー「表示項目と順序」：▲▼で表示順を入れ替える
    { let w = ui.as_weak();
      bk.on_col_move(move |pos, dir| {
          let ui = w.unwrap(); let bk = ui.global::<Backend>();
          let mut o: Vec<i32> = bk.get_d_col_order().iter().collect();
          let (p, q) = (pos as isize, pos as isize + dir as isize);
          if p < 0 || q < 0 || p as usize >= o.len() || q as usize >= o.len() { return; }
          o.swap(p as usize, q as usize);
          bk.set_d_col_order(ModelRc::new(VecModel::from(o)));
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
          // 自局のGL（4桁または6桁。空なら距離・方角は出さない）
          let g = gl_norm(bk.get_d_gl().trim());
          let mut msg = String::new();
          if g.is_empty() || gl_valid(&g) {
              s.cfg.my_gl = g;
              bk.set_my_gl(s.cfg.my_gl.as_str().into());
              refresh_dist(&ui);
          } else {
              msg = "自局のGLの形式が正しくないので、GLは保存しませんでした（例: PM97 または PM97kk）".into();
          }
          let f: Vec<bool> = bk.get_d_col_show().iter().collect();
          let o: Vec<usize> = bk.get_d_col_order().iter().map(|c| c.max(0) as usize).collect();
          if f.len() == NCOL && is_perm(&o) {
              s.cfg.col_show = f.clone();
              s.cfg.col_order = o.clone();
              set_cols(&ui, &o, &f);
          }
          s.cfg.save();
          status(&ui, &msg);
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
    // ヘルプ：FAQ・取扱説明書・ライセンス（文章はアプリに内蔵）
    { let w = ui.as_weak();
      bk.on_show_help(move |i| {
          let ui = w.unwrap(); let bk = ui.global::<Backend>();
          let (title, text) = match i {
              0 => ("FAQ（よくある質問）", FAQ_TEXT),
              1 => ("取扱説明書", MANUAL_TEXT),
              _ => ("ライセンス（MIT）", LICENSE_TEXT),
          };
          bk.set_help_title(title.into());
          bk.set_help_text(text.into());
          bk.set_help_open(true);
      }); }
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_save_pdf(move || {
          let ui = w.unwrap(); let bk = ui.global::<Backend>();
          if bk.get_pdf_busy() { return; }
          let (title, pages, fonts) = {
              let s = a.borrow();
              match &s.doc { Some(d) => (d.job.title.clone(), d.pages.clone(), d.fonts.clone()), None => return }
          };
          bk.set_pdf_busy(true);
          status(&ui, "保存先を選んでください");
          // 保存先ダイアログとPDFの作成は別スレッドで行う（画面の処理を止めない・RefCellを借りたままにしない）
          let weak = ui.as_weak();
          std::thread::spawn(move || {
              let msg = match rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name("logbook.pdf").save_file() {
                  None => String::new(),
                  Some(mut p) => {
                      if p.extension().is_none() { p.set_extension("pdf"); }
                      let _ = weak.clone().upgrade_in_event_loop(|ui| status(&ui, "PDFを作っています…"));
                      match write_pdf(&title, &pages, &fonts, &p) {
                          Ok(()) => format!("PDFを保存しました: {}", p.display()),
                          Err(e) => e,
                      }
                  }
              };
              let _ = weak.upgrade_in_event_loop(move |ui| {
                  ui.global::<Backend>().set_pdf_busy(false);
                  status(&ui, &msg);
              });
          });
      }); }
    // PDF出力画面を閉じる：画像とデータを手放して、キーボードの入力先を元の画面に戻す
    { let (w, a) = (ui.as_weak(), app.clone());
      bk.on_preview_close(move || {
          let ui = w.unwrap(); let bk = ui.global::<Backend>();
          bk.set_preview_open(false);
          bk.set_preview_image(slint::Image::default());
          if let Ok(mut s) = a.try_borrow_mut() { s.doc = None; }
          bump_focus(&ui);
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
    { let a = app.clone();
      bk.on_cols_changed(move || {
          let mut s = a.borrow_mut();
          s.cfg.cols = s.colw.iter().collect();
          s.cfg.cols_ver = 3;
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
                  // 名前を付けて保存：いまの記録を別のファイルに書き出し、以後はそのファイルを使う
                  let mut s = a.borrow_mut();
                  let name = s.file.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string())
                      .unwrap_or_else(|| "log.logbook".to_string());
                  if let Some(mut p) = rfd::FileDialog::new().add_filter("logbook", &["logbook"]).set_file_name(name.as_str()).save_file() {
                      if p.extension().is_none() { p.set_extension("logbook"); }
                      let old = s.file.replace(p.clone());
                      match persist(&s) {
                          Ok(()) => {
                              s.cfg.last_file = Some(p.to_string_lossy().to_string());
                              s.cfg.save();
                              let fname = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                              ui.global::<Backend>().set_file_name(fname.into());
                              status(&ui, "");
                          }
                          Err(e) => { s.file = old; status(&ui, &e); }
                      }
                  }
              }
              (0, 3) => {
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
              (0, 4) => {
                  let mut s = a.borrow_mut();
                  match open_pdf_screen(&ui, &mut s) { Ok(()) => status(&ui, ""), Err(e) => status(&ui, &e) }
              }
              (0, 6) => { let _ = slint::quit_event_loop(); }
              (2, 0) | (2, 1) => { ui.global::<Backend>().set_d_use_api(i == 0); }
              (2, 5) => { let bk = ui.global::<Backend>(); bk.set_d_dark(!bk.get_d_dark()); }
              (2, 6) => { let bk = ui.global::<Backend>(); bk.set_d_clock(!bk.get_d_clock()); }
              (2, 7) => { let bk = ui.global::<Backend>(); bk.set_d_sec(!bk.get_d_sec()); }
              (2, 8) => { let bk = ui.global::<Backend>(); bk.set_d_popup(!bk.get_d_popup()); }
              (3, i) if (0..NCOL as i32).contains(&i) => {
                  let bk = ui.global::<Backend>();
                  let mut f: Vec<bool> = bk.get_d_col_show().iter().collect();
                  if f.len() == NCOL {
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
          if s.cfg.use_api { start_lookup(&ui, s.cfg.api_url.clone(), call); }
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
          for (i, v) in [&rec.freq, &rec.mode, &rec.power, &rec.rst_s, &rec.rst_r].into_iter().enumerate() {
              if !v.is_empty() { s.last[i] = v.clone(); }
          }
          s.cfg.last = s.last.to_vec();
          s.cfg.save();
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
    // ウィンドウを先に表示してイベントループに入ってから読み込む（起動直後の「応答していません」対策）
    #[cfg(windows)]
    {
        ui.show()?;
        fit_to_screen(&ui);
    }
    #[cfg(not(windows))]
    {
        ui.window().set_maximized(true);
        ui.show()?;
    }
    {
        let w = ui.as_weak();
        let a = app.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
            let Some(ui) = w.upgrade() else { return };
            let mut s = a.borrow_mut();
            let last = s.cfg.last_file.clone().map(PathBuf::from).filter(|p| p.exists());
            let opened = match last { Some(p) => open_file(&ui, &mut s, p).is_ok(), None => false };
            if !opened {
                let bk = ui.global::<Backend>();
                bk.set_message("「ファイル」メニューから、ファイルを開くか新規作成してください".into());
                bk.set_screen(1);
            }
        });
    }
    ui.run()
}
