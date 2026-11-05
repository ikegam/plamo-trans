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

unsafe extern "C" {
    fn pt_load_model(path: *const c_char, n_gpu_layers: c_int) -> *mut Model;
    fn pt_new_context(model: *mut Model, n_ctx: u32, n_batch: u32) -> *mut Ctx;
    fn pt_greedy_sampler() -> *mut Sampler;

    fn llama_log_set(cb: extern "C" fn(c_int, *const c_char, *mut c_void), ud: *mut c_void);
    fn llama_backend_init();
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
    fn llama_batch_ext_free(b: *mut Batch);
    fn llama_batch_ext_clear(b: *mut Batch);
    fn llama_batch_ext_add_token(b: *mut Batch, seq: i32, tok: Token) -> i32;
    fn llama_batch_ext_set_pos(b: *mut Batch, idx: i32, pos: *const i32) -> bool;
    fn llama_batch_ext_set_output_logits(b: *mut Batch, idx: i32, v: bool) -> bool;
    fn llama_process(ctx: *mut Ctx, ty: c_int, b: *mut Batch) -> i32;
    fn llama_sampler_sample(s: *mut Sampler, ctx: *mut Ctx, idx: i32) -> Token;
    fn llama_sampler_free(s: *mut Sampler);
    fn llama_free(ctx: *mut Ctx);
    fn llama_model_free(m: *mut Model);
    fn ggml_time_us() -> i64;
}

static VERBOSE: AtomicBool = AtomicBool::new(false);

extern "C" fn log_cb(level: c_int, text: *const c_char, _: *mut c_void) {
    if VERBOSE.load(Ordering::Relaxed) || level == GGML_LOG_LEVEL_ERROR {
        let s = unsafe { std::ffi::CStr::from_ptr(text) };
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

const USAGE: &str = "usage: plamo-trans [options] [text...]   (text from stdin when no text argument)
  -t LANG   target language (default: en if the input contains kana, else ja)
  -s LANG   source language (default: detected by the model)
  -p / -P   polite / plain Japanese      -e  easy Japanese (ja(easy))
  -c TEXT   context: reference text that is not translated
  -g FILE   glossary, one \"source<TAB>target\" per line
  -m FILE   model (default: $PLAMO_MODEL, else
            ${XDG_DATA_HOME:-~/.local/share}/plamo-trans/plamo-3-translate-31b-Q4_K_M.gguf)
  -v        llama.cpp logs and timings on stderr
LANG: en ja zh-hans zh-hant ko fr de es it pt ru ...";

#[derive(Default)]
struct Opts {
    target: Option<String>,
    source: Option<String>,
    form: Option<&'static str>,
    context: Option<String>,
    glossary: Option<String>,
    model: Option<String>,
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
            "-p" => o.form = Some("polite"),
            "-P" => o.form = Some("plain"),
            "-e" => o.target = Some("ja(easy)".into()),
            "-v" => o.verbose = true,
            "-h" | "--help" => return Err(String::new()),
            "--" => o.text.extend(args.by_ref()),
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => o.text.push(a),
        }
    }
    Ok(o)
}

fn default_model() -> String {
    let data = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()));
    format!("{data}/plamo-trans/plamo-3-translate-31b-Q4_K_M.gguf")
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
    let model_path = o.model.clone().or_else(|| std::env::var("PLAMO_MODEL").ok()).unwrap_or_else(default_model);

    VERBOSE.store(o.verbose, Ordering::Relaxed);
    unsafe {
        llama_log_set(log_cb, std::ptr::null_mut());
        llama_backend_init();
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
            let _ = out.write_all(&pending[..valid]);
            let _ = out.flush();
            pending.drain(..valid);
        }
        n_gen += 1;
        batch_set(batch, &[id], pos, true);
        if unsafe { llama_process(ctx, PROCESS_TYPE_DECODE, batch) } != 0 {
            return Err("decode failed".into());
        }
        pos += 1;
    }
    let _ = out.write_all(&pending);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
    let t_end = unsafe { ggml_time_us() };

    if o.verbose {
        let s = |a: i64, b: i64| (b - a) as f64 / 1e6;
        eprintln!(
            "[plamo-trans] target={target} load {:.2} s | prompt {n_prompt} tok {:.2} s ({:.0} t/s) | gen {n_gen} tok {:.2} s ({:.1} t/s)",
            s(t0, t_load),
            s(t_load, t_prompt),
            n_prompt as f64 / s(t_load, t_prompt),
            s(t_prompt, t_end),
            n_gen as f64 / s(t_prompt, t_end)
        );
    }
    unsafe {
        llama_batch_ext_free(batch);
        llama_sampler_free(smpl);
        llama_free(ctx);
        llama_model_free(model);
    }
    Ok(())
}

fn main() -> ExitCode {
    match parse_args() {
        Err(e) => {
            if !e.is_empty() {
                eprintln!("plamo-trans: {e}");
            }
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Ok(o) => match run(o) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("plamo-trans: {e}");
                ExitCode::FAILURE
            }
        },
    }
}
