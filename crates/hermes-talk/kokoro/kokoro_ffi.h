#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void (*KokoroPcmCallback)(const int16_t* samples, size_t count, void* user_data);

typedef struct KokoroEngine KokoroEngine;

typedef struct {
  const char* model_dir;
  const char* prefix_onnx;
  const char* front_rknn;
  const char* tail_onnx;
  const char* vocoder_front_rknn;
  const char* tail_rest_onnx;
  const char* tokens;
  const char* style_npy;
  const char* voice;
  int seq_len;
} KokoroEngineConfig;

/// Returns opaque engine or NULL on failure (message in err_buf if non-null).
KokoroEngine* kokoro_engine_create(const KokoroEngineConfig* cfg, char* err_buf, size_t err_len);

void kokoro_engine_destroy(KokoroEngine* engine);

/// Synthesize text; invokes callback with int16 PCM chunks (24 kHz mono).
/// Returns 0 on success, non-zero on failure.
int kokoro_engine_synthesize_text(
    KokoroEngine* engine,
    const char* text,
    const char* voice,
    float speed,
    int british,
    KokoroPcmCallback callback,
    void* user_data,
    char* err_buf,
    size_t err_len);

#ifdef __cplusplus
}
#endif
