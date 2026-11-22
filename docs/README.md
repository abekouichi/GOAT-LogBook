# GOAT LOGBOOK (Rust + Slint)

Walk版「ログ作成」の仕様を元に、Rust + Slint で作り直したものです。Windows・Linux の両方を想定しています。

## ビルド
    cargo run --release
前提: Rust。初回は依存クレートの取得にネットワークが必要です（`encoding_rs` を追加しました）。

## 配置（実行ファイルと同じフォルダ）
- `setting.json`（自動生成）。市郡区ナンバーはファイルを使いません（下の「市郡区ナンバーとGL」を参照）
- フォントは Windows・Linux とも、本文・見出しは Noto Sans JP（無ければ Noto Sans CJK JP）、等幅（コールサイン）は Windows が Consolas、Linux が Inconsolata（無ければ DejaVu Sans Mono、Liberation Mono、Noto Sans Mono、Ubuntu Mono の順）です。PDFには次の順に探して読み込みます
  1. `setting.json` の `font_regular` / `font_bold` / `font_mono` に書いたパス（最優先）
  2. 実行ファイルと同じ場所の `fonts/` フォルダ
  3. システムのフォントフォルダ（Windows: `Windows\Fonts` とユーザーのフォント、Linux: `/usr/share/fonts`、`~/.local/share/fonts` など。サブフォルダも探します）
- 探すファイル名
  - 本文: `NotoSansJP-Regular.ttf`（可変フォントの `NotoSansJP-VariableFont_wght.ttf`、`NotoSansCJK-Regular.ttc` なども可）
  - 見出し（太字・任意）: `NotoSansJP-Bold.ttf`（無ければ本文と同じ字体）
  - 等幅（任意）: Windows は `consola.ttf`、Linux は `Inconsolata-Regular.ttf`（可変フォントの `Inconsolata[wdth,wght].ttf` なども可）。無ければ `DejaVuSansMono.ttf`、`LiberationMono-Regular.ttf`、`NotoSansMono-Regular.ttf`、`UbuntuMono-R.ttf` の順に探します。どれも無ければ本文と同じ字体（等幅になりません）
- 画面のフォントは、標準が `ui/app.slint` の `Metrics.ui-font`（"Noto Sans JP"）、コールサインなどが等幅（起動時に `src/main.rs` が、PDFと同じ等幅フォントのファイルからフォント名を読んで設定します。Windows は Consolas、Linux は Inconsolata など）です
- Noto Sans JP / Noto Sans CJK / Inconsolata は Google Fonts などから入手できます（SIL Open Font License）
- PDFには TrueType 形式（.ttf）のフォントを推奨します。Noto Sans CJK の .ttc / .otf や Debian系の Inconsolata.otf（CFF形式）は、PDFビューアーによって文字が出ないことがあります

## 入力の補助
- 周波数・モード・出力・RST(S)・RST(R)は、保存したときの値を `setting.json`（`last`）に記録し、次に起動したときも前回の値から始まります
- RST は、前回が2桁なら 59、3桁なら 599 から始まります（前回がないときは 59）。← → で桁を選び、↑で選んだ桁が上がり、↓で下がります（最上位の桁は1〜5、ほかの桁は1〜9）。選んだ桁は反転表示されます。欄に移ったときに選ばれている桁は、2桁なら1の桁、3桁なら10の桁です
- 等幅フォント（Windows: Consolas / Linux: Inconsolata）を使う項目は、日付・開始時刻・終了時刻・コールサイン・RST(S)・RST(R)・周波数・モード・出力・市郡区ナンバー・一括表示・GL・検索GL・距離です。名前・QTH・常置場所・方角・メモは Noto Sans JP です（画面もPDFも同じ）。PDFでは、等幅フォントに無い文字（日本語など）を含む欄だけ Noto Sans JP で描きます

## 市郡区ナンバーとGL
- 市郡区ナンバーは、最初のコールサイン確定のときに、JARLのJCC・JCG・区番号のリスト（`jcc-list.txt`、`jcg-list.txt`、`ku-list.txt`）をネットから読み込んで調べます（文字コードは UTF-8 / Shift_JIS のどちらでも可）。政令指定都市は区ナンバーです
- 入力項目に「GL」（4桁または6桁のグリッドロケーター）があります。設定に「自局のGL」があります
- 「検索GL」は、常置場所の市区町村の位置を、国土地理院の住所検索（`msearch.gsi.go.jp`）で調べた6桁のGLです
- 距離・方角は、自局GLから、入力したGL（なければ検索GL）までを大圏で計算します。同じマスは「-」、隣接8マスとその外側16マスは（）でくくります
- 「ファイル」メニューの「名前を付けて保存…」で、いまの記録を別のファイルに書き出し、以後はそのファイルを使います
- リストモードは、PgUp・PgDn で15行ずつ、Home・End で先頭行・最終行へ移ります。入力モードの Home は日付、End は「保存して次入力」へ、編集モードの Home は日付、End は「保存して終了」へ移ります
- 入力欄の ← → は項目の移動ではなく、カーソルの移動です（日付は年・月・日、時刻は時・分の単位）。項目の移動は Enter・Tab・Shift+Tab です
- 一覧・履歴・PDFの表示項目と表示順は、「設定」→「リスト・履歴の表示項目と順序…」で変えます（`setting.json` の `col_show`・`col_order`）。一覧・履歴の等幅フォントは1.1倍の大きさです。列の幅は、見出し行の境目のドラッグで変えられます（メモも同じ。はじめの幅は半角70文字分です）

## PDF出力
印刷はせず、PDFだけを作ります。「ファイル」→「PDF出力…」で、プレビューと余白の調整ができる画面が開きます。使ったフォントはPDFに埋め込まれます。保存先の選択とPDFの作成は画面とは別に行うので、保存中も画面は止まりません。列の見出しは、列幅を狭めても欠けないようにしています（見出しだけで用紙に収まらないときは、見出しの文字を小さくします）。

## ADIFのインポート・エクスポート
「ファイル」メニューの「ADIFインポート…」「ADIFエクスポート…」で、ADIF（.adi）形式の記録をやり取りします。ログファイルを開いていなくても、すぐにインポートできます（記録は一覧に出るだけで、「名前を付けて保存…」で保存するまで、ファイルに残りません）。
- エクスポートは全記録を日時の昇順で書き出します。インポートは、いま開いているログに追加します（日付・開始時刻・コールサインが同じ記録は重複として除きます）
- ADIFの日付・時刻はUTCなので、パソコンの時刻との間で変換します。ログの時刻が最初からUTCなら、`src/main.rs` の `ADIF_UTC` を `false` にしてください
- 日本語は UTF-8 で書き出します（インポートは UTF-8 / Shift_JIS の両方に対応）。常置場所・市郡区ナンバー・一括表示・検索GLは `APP_GOATLOGBOOK_*` の独自項目で書き出し、他のソフトは読み飛ばします

## 右クリックメニューとカラー
- 検索欄の上の「赤」「黄」「緑」「なし」ボタンで、色で絞り込みます（検索欄に `色:赤` / `色:赤,黄` / `色:なし` が入ります。直接書いても可。複数の色は「または」、ほかの語とは AND）
- リストは、Ctrl＋クリック・Shift＋クリック・Ctrl＋A で複数行を選べます。選んだ行を右クリックして、削除（確認あり）とカラーをまとめて実行できます（編集は1行のときだけ）
- リストの行を右クリックすると、「編集」「削除（確認あり）」「カラー（リセット・赤・黄・緑）」のメニューが出ます
- 一覧・履歴に、色の表示欄があります（はじめは日付の左。「設定」→「リスト・履歴の表示項目と順序…」の「カラー」で、表示・非表示や位置を変えられます）。入力・編集では、終了時刻の右で色を選びます（記録の「色」項目に `red` / `yellow` / `green` で保存。PDFには出ません。ADIFでは `APP_GOATLOGBOOK_COLOR`）

## GOAT Logbookについて
「ヘルプ」→「GOAT Logbookについて」で、バージョン・ビルドの日時・Rust と Slint のバージョン・OS を表示します（「コピー」あり）。ビルドの情報は `build.rs` が埋め込みます（Slint のバージョンは `Cargo.lock` から、コミットは git があるときだけ）。

## ヘルプ（FAQ・取扱説明書・ライセンス）
「ヘルプ」メニューから、アプリの中に画面を開いて表示します。文章は、ビルド時にアプリへ埋め込みます（`docs/faq.txt`、`docs/manual.txt`、`LICENSE`）。取扱説明書（Word）を直したときは、`docs/manual.txt` も直してください。`LICENSE` の著作権者は「anonymous」にしてあります（実名にするときは、`LICENSE` の `anonymous` を直してください）。

## ファイル構成
- `ui/app.slint` : 画面（「編集」メニューの切り取り・コピー・貼り付けは、いまカーソルのある入力欄に対して働きます）
- `src/main.rs`  : 保存・検索・履歴・API取得・設定・PDF出力
- `build.rs`     : Slintのコンパイルと exe アイコン埋め込み
- `assets/`      : アイコン

## 未検証の点
Windows では `cargo build --release` が通ることを確認済みです（Linux は未確認）。JARLのリストの形式と、国土地理院の応答は、実際の通信で確認してください。ビルドで出たエラーは、そのまま貼ってください。
