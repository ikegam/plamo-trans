# plamo-trans

A small command-line translator for
[PLaMo 3 Translate](https://huggingface.co/pfnet/plamo-3-translate-31b) (Preferred Networks), running locally with
[llama.cpp](https://github.com/ggml-org/llama.cpp). There is no server: each run loads the GGUF model, translates,
prints the result and exits.

(Japanese: [README_jp.md](README_jp.md))

```
$ plamo-trans 吾輩は猫である。名前はまだ無い。
I am a cat. I have no name yet.
$ echo "Please let me know if you have any questions." | plamo-trans -p
ご質問があればお知らせください。
$ plamo-trans -t zh-hans 今日はいい天気ですね
今天天气很好呢
```

- Direction is chosen automatically: input with kana is translated into English, anything else into Japanese.
  Use `-t` for other languages.
- Output is streamed as it is generated (greedy decoding).
- Written in Rust. It calls `libllama` directly; the only C is a tiny shim that fills llama.cpp's parameter structs.

## The model is not included

This repository contains no model weights. Download them yourself from Hugging Face and **read the
[PLaMo Community License](https://huggingface.co/pfnet/plamo-3-translate-31b-GGUF) first** (personal and
non-commercial use is free; commercial use needs registration or a separate agreement depending on revenue).

```sh
scripts/download-model.sh              # 31B Q4_K_M (19.5 GB), the default model
scripts/download-model.sh 0.8b         # 0.8B Q4_K_M (0.6 GB), much faster, weaker on hard text
```

The files go to `${XDG_DATA_HOME:-~/.local/share}/plamo-trans/`. Downloads resume after interruption and the
SHA-256 is checked against Hugging Face.

## Build

Requirements: a C/C++ toolchain, CMake, git, Rust (edition 2024), and the CUDA toolkit or Vulkan SDK if you want GPU
offload.

```sh
scripts/build-llama.sh -DGGML_CUDA=ON  # or -DGGML_VULKAN=ON, or nothing for CPU only
cargo build --release
ln -s "$PWD/target/release/plamo-trans" ~/.local/bin/plamo-trans
```

`build-llama.sh` clones llama.cpp into `./llama.cpp` at the commit this tool was tested with (override with
`LLAMA_REF=...`) and builds its shared libraries. The binary finds them through an rpath, so keep this directory where
it is. Set `LLAMA_DIR` to use an existing llama.cpp build tree instead.

## Usage

```
plamo-trans [options] [text...]       text from stdin when no text argument is given
  -t LANG   target language (default: en if the input contains kana, else ja)
  -s LANG   source language (default: detected by the model)
  -p / -P   polite / plain Japanese      -e  easy Japanese (ja(easy))
  -c TEXT   context: reference text that is not translated
  -g FILE   glossary, one "source<TAB>target" per line
  -m FILE   model file (default: $PLAMO_MODEL, else the 31B Q4_K_M in the data directory)
  -v        llama.cpp logs and timings on stderr
```

Language codes are lowercase, e.g. `en ja zh-hans zh-hant ko fr de es it pt ru`.

The prompt is the model's PFML3 translation format, built the same way as the `chat_template.jinja` shipped with the
GGUF. Input and output together are limited to the model's 32K-token training length.

## Notes

- Tested on Linux arm64 (NVIDIA RTX Spark N1X, CUDA 13.4). With the model file in the page cache, the 0.8B model
  starts, translates a sentence and exits in about one second.
- llama.cpp's C API changes often. If you build against another llama.cpp version and it fails to compile or link,
  go back to the pinned commit.

## License

The source code is under the 4-clause BSD license; see [LICENSE](LICENSE). llama.cpp (MIT) is fetched and built
separately and is not part of this repository. The PLaMo 3 Translate models are under the PLaMo Community License.

This tool is not affiliated with Preferred Networks.
