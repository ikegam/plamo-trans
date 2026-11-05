// Thin C shim: the only place that touches llama.cpp parameter structs,
// so the Rust side never has to mirror their memory layout.
#include "llama.h"

struct llama_model * pt_load_model(const char * path, int n_gpu_layers) {
    struct llama_model_params p = llama_model_default_params();
    p.n_gpu_layers = n_gpu_layers;
    return llama_model_load_from_file(path, p);
}

struct llama_context * pt_new_context(struct llama_model * model, unsigned n_ctx, unsigned n_batch) {
    struct llama_context_params p = llama_context_default_params();
    p.n_ctx   = n_ctx;
    p.n_batch = n_batch;
    p.no_perf = 1;
    return llama_init_from_model(model, p);
}

struct llama_sampler * pt_greedy_sampler(void) {
    struct llama_sampler * s = llama_sampler_chain_init(llama_sampler_chain_default_params());
    llama_sampler_chain_add(s, llama_sampler_init_greedy());
    return s;
}
