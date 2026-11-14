# GOAT LOGBOOK (Rust + Slint)

Walk版「ログ作成」の仕様を元に、Rust + Slint で作り直したものです。Windows・Linux の両方を想定しています。

## ビルド
    cargo run --release
前提: Rust。初回は依存クレートの取得にネットワークが必要です。

## 配置（実行ファイルと同じフォルダ）
- `setting.json`（自動生成）、`shigunku_num.json`
- フォントは Windows・Linux とも、本文・見出しは Noto Sans JP（無ければ Noto Sans CJK JP）、等幅（コールサイン）は Windows が Consolas、Linux が Inconsolata です。PDFには次の順に探して読み込みます
  1. `setting.json` の `font_regular` / `font_bold` / `font_mono` に書いたパス（最優先）
  2. 実行ファイルと同じ場所の `fonts/` フォルダ
  3. システムのフォントフォルダ（Windows: `Windows\Fonts` とユーザーのフォント、Linux: `/usr/share/fonts`、`~/.local/share/fonts` など。サブフォルダも探します）
- 探すファイル名
  - 本文: `NotoSansJP-Regular.ttf`（可変フォントの `NotoSansJP-VariableFont_wght.ttf`、`NotoSansCJK-Regular.ttc` なども可）
  - 見出し（太字・任意）: `NotoSansJP-Bold.ttf`（無ければ本文と同じ字体）
  - 等幅（任意）: Windows は `consola.ttf`、Linux は `Inconsolata-Regular.ttf`（可変フォントの `Inconsolata[wdth,wght].ttf` なども可）。無ければ本文と同じ字体
- 画面のフォントは、標準が `ui/app.slint` の `Metrics.ui-font`（"Noto Sans JP"）、コールサインが OS に合わせた等幅（起動時に `src/main.rs` が Windows は Consolas、Linux は Inconsolata に設定）です
- Noto Sans JP / Noto Sans CJK / Inconsolata は Google Fonts などから入手できます（SIL Open Font License）
- PDFには TrueType 形式（.ttf）のフォントを推奨します。Noto Sans CJK の .ttc / .otf や Debian系の Inconsolata.otf（CFF形式）は、PDFビューアーによって文字が出ないことがあります

## 入力の補助
- 周波数・モード・出力・RST(S)・RST(R)は、保存したときの値を `setting.json`（`last`）に記録し、次に起動したときも前回の値から始まります
- RST は、前回が2桁なら 59、3桁なら 599 から始まります（前回がないときは 59）。↓で S の桁（2桁なら1の桁、3桁なら10の桁）が下がり（最小1）、↑で上がります（最大9）
- 等幅フォント（Windows: Consolas / Linux: Inconsolata）を使う項目は、日付・開始時刻・終了時刻・コールサイン・RST(S)・RST(R)・周波数・モード・出力・市郡区ナンバー・一括表示です。名前・QTH・常置場所・メモは Noto Sans JP です（画面もPDFも同じ）。PDFでは、等幅フォントに無い文字（日本語など）を含む欄だけ Noto Sans JP で描きます

## PDF出力
印刷はせず、PDFだけを作ります。「ファイル」→「PDF出力…」で、プレビューと余白の調整ができる画面が開きます。使ったフォントはPDFに埋め込まれます。保存先の選択とPDFの作成は画面とは別に行うので、保存中も画面は止まりません。列の見出しは、列幅を狭めても欠けないようにしています（見出しだけで用紙に収まらないときは、見出しの文字を小さくします）。

## ファイル構成
- `ui/app.slint` : 画面（「編集」メニューの切り取り・コピー・貼り付けは、いまカーソルのある入力欄に対して働きます）
- `src/main.rs`  : 保存・検索・履歴・API取得・設定・PDF出力
- `build.rs`     : Slintのコンパイルと exe アイコン埋め込み
- `assets/`      : アイコン

## 未検証の点
この環境ではコンパイルできていません。ビルドで出たエラーは、そのまま貼ってください。
