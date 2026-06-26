#include "kokoro_ffi.h"

#include <cstring>
#include <string>

#include "g2p.hpp"
#include "kokoro.hpp"
#include "phonemizer.hpp"

struct KokoroEngine {
  kokoro::Engine engine;
};

static void copy_err(char* err_buf, size_t err_len, const std::string& msg) {
  if (!err_buf || err_len == 0) {
    return;
  }
  std::strncpy(err_buf, msg.c_str(), err_len - 1);
  err_buf[err_len - 1] = '\0';
}

KokoroEngine* kokoro_engine_create(const KokoroEngineConfig* cfg, char* err_buf, size_t err_len) {
  if (!cfg || !cfg->vocab_json || !cfg->encoder_onnx || !cfg->har_onnx || !cfg->decoder_path
      || !cfg->voices_dir) {
    copy_err(err_buf, err_len, "kokoro_engine_create: missing required paths");
    return nullptr;
  }
  try {
    std::string espeak = cfg->espeak_data ? cfg->espeak_data : "";
    kokoro::Phonemizer::init(espeak);
    std::string lexicon = cfg->lexicon_dir ? cfg->lexicon_dir : "";
    kokoro::G2P::init(lexicon, espeak);

    kokoro::EngineConfig ecfg;
    ecfg.T_fix = cfg->t_fix > 0 ? cfg->t_fix : 50;
    std::string dec = cfg->decoder_path;
    if (dec.size() >= 5 && dec.substr(dec.size() - 5) == ".rknn") {
      ecfg.decoderWorkers = 3;
    } else {
      ecfg.decoderWorkers = 1;
    }

    auto* eng = new KokoroEngine();
    eng->engine.load(cfg->vocab_json, cfg->encoder_onnx, cfg->har_onnx, cfg->decoder_path,
                     cfg->voices_dir, "", ecfg);
    return eng;
  } catch (const std::exception& e) {
    copy_err(err_buf, err_len, e.what());
    return nullptr;
  } catch (...) {
    copy_err(err_buf, err_len, "kokoro_engine_create: unknown error");
    return nullptr;
  }
}

void kokoro_engine_destroy(KokoroEngine* engine) {
  delete engine;
}

int kokoro_engine_synthesize_text(KokoroEngine* engine, const char* text, const char* voice,
                                  float speed, int british, KokoroPcmCallback callback,
                                  void* user_data, char* err_buf, size_t err_len) {
  if (!engine || !text || !voice || !callback) {
    copy_err(err_buf, err_len, "kokoro_engine_synthesize_text: invalid arguments");
    return 1;
  }
  try {
    engine->engine.synthesizeText(
        text, voice, speed, british != 0,
        [&](const int16_t* data, std::size_t n) { callback(data, n, user_data); });
    return 0;
  } catch (const std::exception& e) {
    copy_err(err_buf, err_len, e.what());
    return 2;
  } catch (...) {
    copy_err(err_buf, err_len, "kokoro_engine_synthesize_text: unknown error");
    return 3;
  }
}
