use std::process::Command;

/// UNIX時刻（秒）→ "YYYY-MM-DDTHH:MM:SSZ"（UTC）
fn iso_utc(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // 日数 → 年月日（Howard Hinnant の civil_from_days）
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, rem % 3600 / 60, rem % 60)
}

fn main() {
    slint_build::compile("ui/app.slint").unwrap();

    // 「GOAT Logbookについて」に出す、ビルドの情報
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    println!("cargo:rustc-env=GOAT_BUILD_DATE={}", iso_utc(secs));
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    if let Ok(o) = Command::new(rustc).arg("-V").output() {
        let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !v.is_empty() { println!("cargo:rustc-env=GOAT_RUSTC={}", v); }
    }
    // Slint のバージョン（Cargo.lock から）
    if let Ok(lock) = std::fs::read_to_string("Cargo.lock") {
        let mut it = lock.lines();
        while let Some(l) = it.next() {
            if l.trim() == "name = \"slint\"" {
                if let Some(v) = it.next().and_then(|l| l.trim().strip_prefix("version = \"")) {
                    println!("cargo:rustc-env=GOAT_SLINT={}", v.trim_end_matches('"'));
                }
                break;
            }
        }
    }
    // git があれば、コミット（なくても可）
    if let Ok(o) = Command::new("git").args(["rev-parse", "HEAD"]).output() {
        if o.status.success() {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !v.is_empty() { println!("cargo:rustc-env=GOAT_COMMIT={}", v); }
        }
    }

    // exe ファイル自体のアイコン（Windows ターゲット時のみ）
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/GOAT_LOGBOOK.ico");
        res.compile().unwrap();
    }
}
