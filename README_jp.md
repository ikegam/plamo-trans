# plamo-trans

Preferred Networks の翻訳モデル [PLaMo 3 Translate](https://huggingface.co/pfnet/plamo-3-translate-31b) を、
[llama.cpp](https://github.com/ggml-org/llama.cpp) を使って手元で動かすコマンドラインの翻訳ツールです。
サーバーは常駐しません。実行するたびに GGUF のモデルを読み込み、翻訳して結果を出し、終了します。

(English: [README.md](README.md))

```
$ plamo-trans 吾輩は猫である。名前はまだ無い。
I am a cat. I have no name yet.
$ echo "Please let me know if you have any questions." | plamo-trans -p
ご質問があればお知らせください。
$ plamo-trans -t zh-hans 今日はいい天気ですね
今天天气很好呢
```

- 翻訳の向きは自動で決まります。入力にかなが含まれていれば英語に、そうでなければ日本語に訳します。
  ほかの言語にするときは `-t` で指定します。
- 訳文は生成した順に表示します(貪欲法でデコードします)。
- Rust で書いています。`libllama` を直接呼びます。C は、llama.cpp の設定用の構造体に値を入れる小さな中継部分だけです。

## モデルは含まれていません

このリポジトリにモデルの重みは入っていません。Hugging Face から各自でダウンロードしてください。
使う前に **[PLaMo Community License](https://huggingface.co/pfnet/plamo-3-translate-31b-GGUF) を読んでください**
(個人利用と非商用利用は無料です。商用利用は、売上の規模によって登録か個別の契約が必要です)。

```sh
scripts/download-model.sh              # 8B  Q4_K_M(5.3 GB)。既定で使うモデル
scripts/download-model.sh 2b           # 2B  Q4_K_M(1.8 GB)。-f 用
scripts/download-model.sh 0.8b         # 0.8B Q4_K_M(0.6 GB)。-ff 用
scripts/download-model.sh 31b          # 31B Q4_K_M(19.5 GB)。-d 用
```

使うサイズだけ入れれば十分です。保存先は `${XDG_DATA_HOME:-~/.local/share}/plamo-trans/` です。途中で切れても続きからダウンロードし、
最後に Hugging Face の SHA-256 と照合します。

## ビルド

必要なもの: C/C++ のコンパイラ、CMake、git、Rust(edition 2024)。GPU を使う場合は CUDA toolkit か Vulkan SDK。

```sh
scripts/build-llama.sh -DGGML_CUDA=ON  # Vulkan なら -DGGML_VULKAN=ON、CPU だけなら指定なし
cargo build --release
ln -s "$PWD/target/release/plamo-trans" ~/.local/bin/plamo-trans
```

`build-llama.sh` は llama.cpp を `./llama.cpp` に clone し、動作を確認したコミットに合わせて
(`LLAMA_REF=...` で変更できます)共有ライブラリをビルドします。実行ファイルは rpath でこのライブラリを探すので、
このディレクトリは移動しないでください。既存の llama.cpp のビルドを使うときは `LLAMA_DIR` を指定します。

## 使い方

```
plamo-trans [オプション] [文章...]     文章を引数で渡さなければ標準入力から読みます
  -t 言語    訳し先の言語(既定: 入力にかながあれば en、なければ ja)
  -s 言語    原文の言語(既定: モデルが判断)
  -p / -P    丁寧体 / 常体の日本語      -e  やさしい日本語(ja(easy))
  -c 文章    文脈: 訳さずに参考として渡す文章
  -g ファイル 用語集。1 行に「原語<TAB>訳語」
  -ff        0.8B モデル(最速)     -f  2B(速い)     -d  31B(じっくり)
  -m ファイル モデルのファイル(サイズの指定より優先)
  -v         llama.cpp のログと所要時間を標準エラーに出す
```

サイズを指定しなければ 8B を使います(`$PLAMO_MODEL` があればそれを使います)。8B が入っていなければ、
入っている中で近いサイズ(2B、31B、0.8B の順)を使い、標準エラーにその旨を出します。サイズを指定したときはそのサイズだけを使い、
入っていなければダウンロードの方法を表示します。各サイズでは Q4_K_M を優先し、なければデータディレクトリにある別の量子化版を使います。

言語コードは小文字で指定します(例: `en ja zh-hans zh-hant ko fr de es it pt ru`)。

プロンプトはモデルの翻訳用形式 PFML3 で、GGUF に付いている `chat_template.jinja` と同じ組み立て方です。
入力と出力を合わせて、モデルの学習時の長さ 32K トークンまでです。

## 補足

- Linux arm64(NVIDIA RTX Spark N1X、CUDA 13.4)で動作を確認しています。モデルのファイルがページキャッシュに
  載っていれば、起動・1 文の翻訳・終了までが 0.8B で約 1 秒、8B で約 2 秒(毎秒 45 トークン)、31B で 4〜5 秒(毎秒 12.5 トークン)です。
- llama.cpp の C API はよく変わります。別の版の llama.cpp でビルドやリンクに失敗したら、固定したコミットに戻してください。

## ライセンス

ソースコードは 4 条項 BSD ライセンスです([LICENSE](LICENSE))。llama.cpp(MIT)は別に取得してビルドするもので、
このリポジトリには含みません。PLaMo 3 Translate のモデルは PLaMo Community License です。

このツールは Preferred Networks とは関係ありません。
