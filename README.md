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
scripts/download-model.sh              # 8B  Q4_K_M (5.3 GB), the default
scripts/download-model.sh 2b           # 2B  Q4_K_M (1.8 GB), for -f
scripts/download-model.sh 0.8b         # 0.8B Q4_K_M (0.6 GB), for -ff
scripts/download-model.sh 31b          # 31B Q4_K_M (19.5 GB), for -d
```

You only need the sizes you use. The files go to `${XDG_DATA_HOME:-~/.local/share}/plamo-trans/`. Downloads resume after interruption and the
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
  -ff       0.8B model (fastest)     -f  2B (fast)     -d  31B (deep)
  -m FILE   model file (overrides the size options)
  -v        llama.cpp logs and timings on stderr
```

Without a size option the 8B model is used (or `$PLAMO_MODEL` if set). If 8B is not installed, the nearest
installed size is used instead (2B, 31B, 0.8B in that order) with a note on stderr. A size option asks for exactly
that size and tells you how to download it when it is missing. For each size, Q4_K_M is preferred, then any other
quantisation found in the data directory.

Language codes are lowercase, e.g. `en ja zh-hans zh-hant ko fr de es it pt ru`.

The prompt is the model's PFML3 translation format, built the same way as the `chat_template.jinja` shipped with the
GGUF. Input and output together are limited to the model's 32K-token training length.

## Notes

- Tested on Linux arm64 (NVIDIA RTX Spark N1X, CUDA 13.4). With the model file in the page cache, one sentence
  (start, translate, exit) takes about 1 s with 0.8B, 2 s with 8B (45 tokens/s) and 4–5 s with 31B (12.5 tokens/s).
- llama.cpp's C API changes often. If you build against another llama.cpp version and it fails to compile or link,
  go back to the pinned commit.

## License

The source code is under the 4-clause BSD license; see [LICENSE](LICENSE). llama.cpp (MIT) is fetched and built
separately and is not part of this repository. The PLaMo 3 Translate models are under the PLaMo Community License.

This tool is not affiliated with Preferred Networks.
