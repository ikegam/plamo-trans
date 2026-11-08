//! plamo-trans: one-shot translator for PLaMo 3 Translate (GGUF) on top of libllama.
//! No server: load the model, build the PFML3 prompt, decode greedily, stream the result, exit.

use std::ffi::{CString, c_char, c_int, c_void};
use std::io::{IsTerminal, Read, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

// ---- libllama FFI (only functions with scalar/pointer arguments; structs go through shim.c) ----

#[repr(C)]
struct Opaque {
    _p: [u8; 0],
}
type Model = Opaque;
type Ctx = Opaque;
type Vocab = Opaque;
type Batch = Opaque;
type Sampler = Opaque;
type Token = i32;

const PROCESS_TYPE_DECODE: c_int = 1;
const GGML_LOG_LEVEL_ERROR: c_int = 4;
/// PLaMo 3 Translate is trained on up to 32K tokens (input + output).
const MAX_CTX: i32 = 32768;
/// Error marker for a closed stdout; exits quietly with 141 (128 + SIGPIPE) like other Unix tools.
const BROKEN_PIPE: &str = "\0broken pipe";

unsafe extern "C" {
    fn pt_load_model(path: *const c_char, n_gpu_layers: c_int) -> *mut Model;
    fn pt_new_context(model: *mut Model, n_ctx: u32, n_batch: u32) -> *mut Ctx;
    fn pt_greedy_sampler() -> *mut Sampler;
    fn pt_light_init();

    fn llama_log_set(cb: extern "C" fn(c_int, *const c_char, *mut c_void), ud: *mut c_void);
    fn llama_model_get_vocab(model: *const Model) -> *const Vocab;
    fn llama_tokenize(
        vocab: *const Vocab,
        text: *const c_char,
        len: i32,
        tokens: *mut Token,
        n_max: i32,
        add_special: bool,
        parse_special: bool,
    ) -> i32;
    fn llama_token_to_piece(
        vocab: *const Vocab,
        tok: Token,
        buf: *mut c_char,
        len: i32,
        lstrip: i32,
        special: bool,
    ) -> i32;
    fn llama_vocab_is_eog(vocab: *const Vocab, tok: Token) -> bool;
    fn llama_batch_ext_init(ctx: *mut Ctx) -> *mut Batch;
    fn llama_batch_ext_clear(b: *mut Batch);
    fn llama_batch_ext_add_token(b: *mut Batch, seq: i32, tok: Token) -> i32;
    fn llama_batch_ext_set_pos(b: *mut Batch, idx: i32, pos: *const i32) -> bool;
    fn llama_batch_ext_set_output_logits(b: *mut Batch, idx: i32, v: bool) -> bool;
    fn llama_process(ctx: *mut Ctx, ty: c_int, b: *mut Batch) -> i32;
    fn llama_sampler_sample(s: *mut Sampler, ctx: *mut Ctx, idx: i32) -> Token;
    fn ggml_time_us() -> i64;
    fn _exit(code: c_int) -> !;
    fn fork() -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn read(fd: c_int, buf: *mut c_void, n: usize) -> isize;
    fn write(fd: c_int, buf: *const c_void, n: usize) -> isize;
    fn close(fd: c_int) -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    #[cfg(target_os = "linux")]
    fn prctl(option: c_int, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> c_int;
    fn getppid() -> c_int;
    fn getpid() -> c_int;
}

static VERBOSE: AtomicBool = AtomicBool::new(false);
static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

extern "C" fn log_cb(level: c_int, text: *const c_char, _: *mut c_void) {
    if VERBOSE.load(Ordering::Relaxed) || level == GGML_LOG_LEVEL_ERROR {
        let s = unsafe { std::ffi::CStr::from_ptr(text) };
        if VERBOSE.load(Ordering::Relaxed) {
            let ms = START.get_or_init(std::time::Instant::now).elapsed().as_millis();
            let _ = write!(std::io::stderr(), "{ms:>6} ");
        }
        let _ = std::io::stderr().write_all(s.to_bytes());
    }
}

fn tokenize(vocab: *const Vocab, text: &str, add_special: bool) -> Vec<Token> {
    let (p, len) = (text.as_ptr().cast(), text.len() as i32);
    let n = -unsafe { llama_tokenize(vocab, p, len, std::ptr::null_mut(), 0, add_special, true) };
    let mut v = vec![0; n.max(0) as usize];
    let r = unsafe { llama_tokenize(vocab, p, len, v.as_mut_ptr(), n, add_special, true) };
    if r < 0 { Vec::new() } else { v }
}

fn single_token(vocab: *const Vocab, text: &str) -> Option<Token> {
    match tokenize(vocab, text, false).as_slice() {
        [t] => Some(*t),
        _ => None,
    }
}

fn batch_set(b: *mut Batch, toks: &[Token], pos0: i32, logits_last: bool) {
    unsafe {
        llama_batch_ext_clear(b);
        for (i, &t) in toks.iter().enumerate() {
            let idx = llama_batch_ext_add_token(b, 0, t);
            let pos = pos0 + i as i32;
            llama_batch_ext_set_pos(b, idx, &pos);
        }
        if logits_last {
            llama_batch_ext_set_output_logits(b, toks.len() as i32 - 1, true);
        }
    }
}

// ---- options and prompt ----

const USAGE: &str = "usage: trs [options] [text...]   (text from stdin when no text argument)
  -t LANG   target language (default: en if the input contains kana, else ja)
  -s LANG   source language (default: detected by the model)
  -p / -P   polite / plain Japanese      -e  easy Japanese (ja(easy))
  -c TEXT   context: reference text that is not translated
  -g FILE   glossary, one \"source<TAB>target\" per line
  model size (default 8b; if 8b is not installed, the nearest installed size is used):
  -ff       0.8b (fastest)    -f  2b (fast)    -d  31b (deep)
  -m FILE   model file (overrides the size options; default also $PLAMO_MODEL)
            models live in ${XDG_DATA_HOME:-~/.local/share}/plamo-trans/
  -v        llama.cpp logs and timings on stderr
  --download SIZE [QUANT]   download a model (0.8b 2b 8b 31b) from Hugging Face
  -V, --version
LANG: en ja zh-hans zh-hant ko fr de es it pt ru ...";

#[derive(Default)]
struct Opts {
    target: Option<String>,
    source: Option<String>,
    form: Option<&'static str>,
    context: Option<String>,
    glossary: Option<String>,
    model: Option<String>,
    size: Option<&'static str>,
    verbose: bool,
    text: Vec<String>,
}

fn parse_args() -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "-t" => o.target = Some(val("-t")?),
            "-s" => o.source = Some(val("-s")?),
            "-c" => o.context = Some(val("-c")?),
            "-g" => o.glossary = Some(val("-g")?),
            "-m" => o.model = Some(val("-m")?),
            "-ff" => o.size = Some("0.8b"),
            "-f" => o.size = Some("2b"),
            "-d" => o.size = Some("31b"),
            "-p" => o.form = Some("polite"),
            "-P" => o.form = Some("plain"),
            "-e" => o.target = Some("ja(easy)".into()),
            "-v" => o.verbose = true,
            "-h" | "--help" => return Err(String::new()),
            "-V" | "--version" => {
                println!("trs (plamo-trans) {} llama.cpp {}", env!("CARGO_PKG_VERSION"), LLAMA_COMMIT.trim());
                std::process::exit(0);
            }
            "--download" => {
                let rest: Vec<String> = args.collect();
                std::process::exit(download(&rest));
            }
            "--" => o.text.extend(args.by_ref()),
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => o.text.push(a),
        }
    }
    Ok(o)
}

const DEFAULT_SIZE: &str = "8b";
/// Fallback order when the default size is not installed (nearest first, lighter first on ties).
const FALLBACK: [&str; 4] = ["8b", "2b", "31b", "0.8b"];

fn model_dir() -> std::path::PathBuf {
    let data = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()));
    std::path::Path::new(&data).join("plamo-trans")
}

/// Installed GGUF for a size: Q4_K_M if present, else any other quantisation of that size.
fn find_model(size: &str) -> Option<String> {
    let dir = model_dir();
    let q4 = dir.join(format!("plamo-3-translate-{size}-Q4_K_M.gguf"));
    if q4.is_file() {
        return Some(q4.to_string_lossy().into_owned());
    }
    let prefix = format!("plamo-3-translate-{size}-");
    let mut found: Vec<String> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with(&prefix) && n.ends_with(".gguf")
        })
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    found.sort();
    found.pop()
}

const LLAMA_COMMIT: &str = include_str!("../llama.cpp.commit");
const DOWNLOAD_SCRIPT: &str = include_str!("../scripts/download-model.sh");

fn download_hint(size: &str) -> String {
    format!("the {size} model is not installed; download it with\n  trs --download {size}")
}

/// `trs --download SIZE [QUANT]`: run the embedded scripts/download-model.sh.
fn download(args: &[String]) -> i32 {
    if args.is_empty() || args.len() > 2 {
        eprintln!("usage: trs --download SIZE [QUANT]   SIZE: 0.8b 2b 8b 31b");
        return 2;
    }
    match std::process::Command::new("sh").arg("-c").arg(DOWNLOAD_SCRIPT).arg("download-model").args(args).status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("trs: cannot run sh: {e}");
            1
        }
    }
}

fn resolve_model(o: &Opts) -> Result<String, String> {
    if let Some(m) = &o.model {
        return Ok(m.clone());
    }
    if let Some(size) = o.size {
        return find_model(size).ok_or_else(|| download_hint(size));
    }
    if let Ok(m) = std::env::var("PLAMO_MODEL") {
        return Ok(m);
    }
    for size in FALLBACK {
        if let Some(m) = find_model(size) {
            if size != DEFAULT_SIZE {
                eprintln!("trs: {DEFAULT_SIZE} model not installed, using {size}");
            }
            return Ok(m);
        }
    }
    Err(download_hint(DEFAULT_SIZE))
}

/// Remove reserved PFML markers so user text cannot inject prompt structure.
fn clean(s: &str) -> String {
    s.replace("<|plamo:", "").trim().to_string()
}

fn has_kana(s: &str) -> bool {
    s.chars().any(|c| ('\u{3040}'..='\u{30ff}').contains(&c))
}

/// PFML3 prompt, same as the GGUF sidecar chat_template.jinja. BOS is added by the tokenizer.
fn build_prompt(o: &Opts, text: &str, target: &str) -> Result<String, String> {
    let mut p = String::from("<|plamo:tag|>dataset<|plamo:msg|>translation");
    if let Some(c) = &o.context {
        p += &format!("<|plamo:tag|>context<|plamo:msg|>{}", clean(c));
    }
    if let Some(g) = &o.glossary {
        let body = std::fs::read_to_string(g).map_err(|e| format!("{g}: {e}"))?;
        let items: String = body
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('\t'))
            .map(|(s, t)| (clean(s), clean(t)))
            .filter(|(s, t)| !s.is_empty() && !t.is_empty())
            .map(|(s, t)| format!("<|plamo:index|>{s}<|plamo:item|>{t}"))
            .collect();
        if !items.is_empty() {
            p += "<|plamo:tag|>glossary";
            p += &items;
        }
    }
    p += "<|plamo:tag|>input";
    if let Some(s) = &o.source {
        p += &format!("<|plamo:key|>lang<|plamo:val|>{s}");
    }
    p += &format!("<|plamo:msg|>{text}<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>{target}");
    if let Some(f) = o.form {
        p += &format!("<|plamo:key|>form<|plamo:val|>{f}");
    }
    p += "<|plamo:msg|>";
    Ok(p)
}

// ---- main ----

fn run(o: Opts) -> Result<(), String> {
    let text = if o.text.is_empty() {
        if std::io::stdin().is_terminal() {
            eprintln!("(reading text from stdin, end with Ctrl-D)");
        }
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
        s
    } else {
        o.text.join(" ")
    };
    let text = clean(&text);
    if text.is_empty() {
        return Err("no text to translate".into());
    }
    let target = o.target.clone().unwrap_or_else(|| if has_kana(&text) { "en" } else { "ja" }.into());
    if o.form.is_some() && target != "ja" {
        return Err("-p/-P need target ja".into());
    }
    let prompt = build_prompt(&o, &text, &target)?;
    let model_path = resolve_model(&o)?;

    VERBOSE.store(o.verbose, Ordering::Relaxed);
    unsafe {
        llama_log_set(log_cb, std::ptr::null_mut());
        pt_light_init();
    }
    let t0 = unsafe { ggml_time_us() };
    let cpath = CString::new(model_path.clone()).map_err(|e| e.to_string())?;
    let model = unsafe { pt_load_model(cpath.as_ptr(), 999) };
    if model.is_null() {
        return Err(format!("cannot load {model_path}"));
    }
    let vocab = unsafe { llama_model_get_vocab(model) };

    let ptoks = tokenize(vocab, &prompt, true);
    if ptoks.is_empty() {
        return Err("tokenize failed".into());
    }
    let n_prompt = ptoks.len() as i32;
    let n_ctx = (n_prompt + (n_prompt * 3).max(256)).min(MAX_CTX);
    if n_prompt + 16 > n_ctx {
        return Err(format!("input too long ({n_prompt} tokens, limit ~{MAX_CTX} incl. output)"));
    }
    let n_batch = n_prompt.min(2048);
    let ctx = unsafe { pt_new_context(model, n_ctx as u32, n_batch as u32) };
    if ctx.is_null() {
        return Err("cannot create context".into());
    }
    let smpl = unsafe { pt_greedy_sampler() };
    let stops: Vec<Token> =
        ["<|plamo:tag|>", "<|plamo:bos|>"].iter().filter_map(|s| single_token(vocab, s)).collect();
    let batch = unsafe { llama_batch_ext_init(ctx) };
    let t_load = unsafe { ggml_time_us() };

    // prompt, in chunks of n_batch
    let mut pos = 0;
    for chunk in ptoks.chunks(n_batch as usize) {
        let last = pos + chunk.len() as i32 == n_prompt;
        batch_set(batch, chunk, pos, last);
        if unsafe { llama_process(ctx, PROCESS_TYPE_DECODE, batch) } != 0 {
            return Err("prompt eval failed".into());
        }
        pos += chunk.len() as i32;
    }
    let t_prompt = unsafe { ggml_time_us() };

    // greedy decode, streaming complete UTF-8 only
    let mut out = std::io::stdout().lock();
    let mut pending: Vec<u8> = Vec::new();
    let mut started = false;
    let mut n_gen = 0;
    while pos < n_ctx {
        let id = unsafe { llama_sampler_sample(smpl, ctx, -1) };
        if stops.contains(&id) || unsafe { llama_vocab_is_eog(vocab, id) } {
            break;
        }
        let mut buf = [0u8; 256];
        let n = unsafe { llama_token_to_piece(vocab, id, buf.as_mut_ptr().cast(), buf.len() as i32, 0, false) };
        if n > 0 {
            pending.extend_from_slice(&buf[..n as usize]);
            if !started {
                let skip = pending.iter().take_while(|b| b.is_ascii_whitespace()).count();
                pending.drain(..skip);
                started = !pending.is_empty();
            }
            let valid = match std::str::from_utf8(&pending) {
                Ok(_) => pending.len(),
                Err(e) => e.valid_up_to(),
            };
            // stop at once when the reader is gone (e.g. `| head`)
            out.write_all(&pending[..valid]).and_then(|_| out.flush()).map_err(|_| BROKEN_PIPE.to_string())?;
            pending.drain(..valid);
        }
        n_gen += 1;
        batch_set(batch, &[id], pos, true);
        if unsafe { llama_process(ctx, PROCESS_TYPE_DECODE, batch) } != 0 {
            return Err("decode failed".into());
        }
        pos += 1;
    }
    out.write_all(&pending)
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush())
        .map_err(|_| BROKEN_PIPE.to_string())?;
    let t_end = unsafe { ggml_time_us() };

    if o.verbose {
        let s = |a: i64, b: i64| (b - a) as f64 / 1e6;
        eprintln!(
            "[trs] target={target} load {:.2} s | prompt {n_prompt} tok {:.2} s ({:.0} t/s) | gen {n_gen} tok {:.2} s ({:.1} t/s)",
            s(t0, t_load),
            s(t_load, t_prompt),
            n_prompt as f64 / s(t_load, t_prompt),
            s(t_prompt, t_end),
            n_gen as f64 / s(t_prompt, t_end)
        );
    }
    // Output is flushed. The model, context and CUDA state are not freed: the worker process _exit()s and
    // the OS reclaims them in the background (see main()).
    let _ = (batch, smpl, ctx, model);
    Ok(())
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
/// Translate in a forked worker and return to the shell as soon as the worker reports its result.
///
/// Tearing down a process that holds a CUDA context and gigabytes of GPU/pinned memory takes the kernel
/// 0.1-0.2 s, and the shell waits for that both on a normal exit and on Ctrl-C. The parent here never touches
/// CUDA, so it exits at once: after the worker has flushed its output and written its exit code to a pipe,
/// or immediately on Ctrl-C (SIGINT goes to the whole foreground process group; the worker dies too and its
/// teardown finishes in the background). The fork happens before any CUDA initialisation.
fn run_forked(o: Opts) -> ExitCode {
    let mut fds = [0 as c_int; 2];
    if unsafe { pipe(fds.as_mut_ptr()) } != 0 {
        return report(run(o));
    }
    let parent = unsafe { getpid() };
    let pid = unsafe { fork() };
    if pid < 0 {
        return report(run(o));
    }
    if pid == 0 {
        // worker: die with the parent (e.g. when only the parent is killed), PR_SET_PDEATHSIG = 1, SIGKILL = 9
        unsafe {
            #[cfg(target_os = "linux")]
            prctl(1, 9, 0, 0, 0);
            if getppid() != parent {
                _exit(130);
            }
            close(fds[0]);
        }
        let code: u8 = match run(o) {
            Ok(()) => 0,
            Err(e) if e == BROKEN_PIPE => 141,
            Err(e) => {
                eprintln!("trs: {e}");
                1
            }
        };
        let _ = std::io::stdout().flush();
        unsafe {
            write(fds[1], (&code as *const u8).cast(), 1);
            _exit(code as c_int)
        }
    }
    // parent: wait for the worker's exit code, not for its exit
    unsafe { close(fds[1]) };
    let mut code = 0u8;
    if unsafe { read(fds[0], (&mut code as *mut u8).cast(), 1) } == 1 {
        return ExitCode::from(code);
    }
    // the worker died without reporting (killed or crashed): mirror its status
    let mut status: c_int = 0;
    unsafe { waitpid(pid, &mut status, 0) };
    let (exited, sig) = (status & 0x7f == 0, status & 0x7f);
    ExitCode::from(if exited { ((status >> 8) & 0xff) as u8 } else { 128u8.wrapping_add(sig as u8) })
}

fn report(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e == BROKEN_PIPE => ExitCode::from(141),
        Err(e) => {
            eprintln!("trs: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    START.get_or_init(std::time::Instant::now);
    match parse_args() {
        Err(e) => {
            if !e.is_empty() {
                eprintln!("trs: {e}");
            }
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        // macOS: no fork. Metal compiles its shaders through XPC (MTLCompilerService), which does not work in a
        // forked child, and there is no CUDA teardown to avoid.
        #[cfg(target_os = "macos")]
        Ok(o) => report(run(o)),
        #[cfg(not(target_os = "macos"))]
        Ok(o) => run_forked(o),
    }
}
