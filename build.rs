fn main() {
    slint_build::compile("ui/app.slint").unwrap();
    // exe ファイル自体のアイコン（Windows ターゲット時のみ）
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/GOAT_LOGBOOK.ico");
        res.compile().unwrap();
    }
}
