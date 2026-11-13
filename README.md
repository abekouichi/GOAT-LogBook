# GOAT LOGBOOK (Rust + Slint)

Walk版「ログ作成」の仕様を元に、Rust + Slint で作り直したものです。Windows・Linux の両方を想定しています。

## ビルド
    cargo run --release
前提: Rust。初回は依存クレートの取得にネットワークが必要です。

## 配置（実行ファイルと同じフォルダ）
- `setting.json`（自動生成）、`shigunku_num.json`
- `fonts/` フォルダ（PDF出力に必要）
  - 必須: `NotoSansJP-Regular.ttf`
  - 任意: `NotoSansJP-Bold.ttf`（見出し用の太字）、`NotoSansMono-Regular.ttf`（コールサイン用の等幅）
  - Noto Sans JP / Noto Sans Mono は Google Fonts から入手できます（SIL Open Font License。PDFへの埋め込みが許可されています）
  - 別の場所のフォントを使うときは、`setting.json` の `font_regular` / `font_bold` / `font_mono` にパスを書きます

## PDF出力
印刷はせず、PDFだけを作ります。「ファイル」→「PDF出力…」で、プレビューと余白の調整ができる画面が開きます。使ったフォントはPDFに埋め込まれます。

## ファイル構成
- `ui/app.slint` : 画面
- `src/main.rs`  : 保存・検索・履歴・API取得・設定・PDF出力
- `build.rs`     : Slintのコンパイルと exe アイコン埋め込み
- `assets/`      : アイコン

## 未検証の点
この環境ではコンパイルできていません。ビルドで出たエラーは、そのまま貼ってください。
