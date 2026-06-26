#include "kokoro_ffi.h"

#include <algorithm>
#include <cmath>
#include <cctype>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <functional>
#include <memory>
#include <optional>
#include <sstream>
#include <string>
#include <unordered_map>
#include <vector>

#include <onnxruntime_cxx_api.h>
#include <rknn_api.h>

#include "npy.hpp"

namespace {

constexpr int kStyleDim = 256;
constexpr int kSampleRate = 24000;
constexpr const char* kDecoderInput = "/MatMul_1_output_0";
constexpr const char* kStyleSlice = "/Slice_2_output_0";
constexpr const char* kFrontOutput = "/decoder/decode.3/Mul_output_0";
constexpr const char* kVocoderFrontOutput = "/decoder/generator/Add_5_output_0";

static void copy_err(char* err_buf, size_t err_len, const std::string& msg) {
  if (!err_buf || err_len == 0) return;
  std::strncpy(err_buf, msg.c_str(), err_len - 1);
  err_buf[err_len - 1] = '\0';
}

static bool file_exists(const std::string& path) {
  std::ifstream f(path, std::ios::binary);
  return f.good();
}

static std::string resolve_path(const char* base, const char* rel) {
  if (!rel || !rel[0]) return {};
  std::string p(rel);
  if (p[0] == '/') return p;
  if (!base || !base[0]) return p;
  std::string b(base);
  if (b.back() == '/') return b + p;
  return b + "/" + p;
}

static void for_each_utf8(const std::string& s, const std::function<void(const std::string&)>& fn) {
  for (size_t i = 0; i < s.size();) {
    unsigned char c = static_cast<unsigned char>(s[i]);
    size_t len = 1;
    if ((c & 0x80) == 0) {
      len = 1;
    } else if ((c & 0xE0) == 0xC0) {
      len = 2;
    } else if ((c & 0xF0) == 0xE0) {
      len = 3;
    } else if ((c & 0xF8) == 0xF0) {
      len = 4;
    }
    if (i + len > s.size()) len = 1;
    fn(s.substr(i, len));
    i += len;
  }
}

class Tokenizer {
 public:
  void load(const std::string& path) {
    std::ifstream f(path);
    if (!f) throw std::runtime_error("cannot open tokens: " + path);
    token_to_id_.clear();
    std::string line;
    int line_no = 0;
    while (std::getline(f, line)) {
      if (line.empty()) continue;
      std::istringstream iss(line);
      std::string token;
      int idx = line_no;
      if (iss >> token) {
        std::string extra;
        if (iss >> extra) {
          try {
            idx = std::stoi(extra);
          } catch (...) {
            idx = line_no;
          }
        }
        token_to_id_[token] = idx;
      }
      ++line_no;
    }
    pad_id_ = first_id({" ", "_"}, 0);
    bos_id_ = optional_id({"^"});
    eos_id_ = optional_id({"$"});
    unk_id_ = optional_id({"UNK"});
  }

  std::pair<std::vector<int64_t>, int> encode(const std::string& text, int seq_len) const {
    std::vector<int64_t> ids;
    if (bos_id_) ids.push_back(*bos_id_);
    for_each_utf8(text, [&](const std::string& ch) {
      int token_id = -1;
      if (ch.size() == 1 && std::isspace(static_cast<unsigned char>(ch[0]))) {
        if (auto it = token_to_id_.find(" "); it != token_to_id_.end()) token_id = it->second;
        else if (auto it = token_to_id_.find("_"); it != token_to_id_.end()) token_id = it->second;
      } else {
        auto it = token_to_id_.find(ch);
        if (it != token_to_id_.end()) token_id = it->second;
        else if (unk_id_) token_id = *unk_id_;
      }
      if (token_id >= 0) ids.push_back(token_id);
    });
    if (eos_id_) ids.push_back(*eos_id_);
    int actual = static_cast<int>(std::min(ids.size(), static_cast<size_t>(seq_len)));
    std::vector<int64_t> arr(static_cast<size_t>(seq_len), pad_id_);
    for (int i = 0; i < actual; ++i) arr[static_cast<size_t>(i)] = ids[static_cast<size_t>(i)];
    return {arr, actual};
  }

 private:
  std::unordered_map<std::string, int> token_to_id_;
  int pad_id_ = 0;
  std::optional<int> bos_id_;
  std::optional<int> eos_id_;
  std::optional<int> unk_id_;

  std::optional<int> optional_id(std::initializer_list<const char*> tokens) const {
    for (auto t : tokens) {
      auto it = token_to_id_.find(t);
      if (it != token_to_id_.end()) return it->second;
    }
    return std::nullopt;
  }

  int first_id(std::initializer_list<const char*> tokens, int fallback) const {
    auto v = optional_id(tokens);
    return v ? *v : fallback;
  }
};

class RknnRunner {
 public:
  RknnRunner() = default;
  RknnRunner(const RknnRunner&) = delete;
  RknnRunner& operator=(const RknnRunner&) = delete;
  RknnRunner(RknnRunner&& o) noexcept { *this = std::move(o); }
  RknnRunner& operator=(RknnRunner&& o) noexcept {
    if (this != &o) {
      destroy();
      ctx_ = o.ctx_;
      in_attrs_ = std::move(o.in_attrs_);
      out_attrs_ = std::move(o.out_attrs_);
      o.ctx_ = 0;
    }
    return *this;
  }
  ~RknnRunner() { destroy(); }

  void load(const std::string& path) {
    destroy();
    std::ifstream f(path, std::ios::binary);
    if (!f) throw std::runtime_error("cannot open rknn: " + path);
    f.seekg(0, std::ios::end);
    std::vector<uint8_t> buf(static_cast<size_t>(f.tellg()));
    f.seekg(0, std::ios::beg);
    f.read(reinterpret_cast<char*>(buf.data()), static_cast<std::streamsize>(buf.size()));
    int ret = rknn_init(&ctx_, buf.data(), buf.size(), 0, nullptr);
    if (ret != RKNN_SUCC) throw std::runtime_error("rknn_init failed: " + std::to_string(ret));
    rknn_input_output_num io_num{};
    ret = rknn_query(ctx_, RKNN_QUERY_IN_OUT_NUM, &io_num, sizeof(io_num));
    if (ret != RKNN_SUCC) throw std::runtime_error("rknn_query io num failed");
    in_attrs_.resize(io_num.n_input);
    out_attrs_.resize(io_num.n_output);
    for (uint32_t i = 0; i < io_num.n_input; ++i) {
      in_attrs_[i].index = i;
      ret = rknn_query(ctx_, RKNN_QUERY_INPUT_ATTR, &in_attrs_[i], sizeof(rknn_tensor_attr));
      if (ret != RKNN_SUCC) throw std::runtime_error("rknn_query input attr failed");
    }
    for (uint32_t i = 0; i < io_num.n_output; ++i) {
      out_attrs_[i].index = i;
      ret = rknn_query(ctx_, RKNN_QUERY_OUTPUT_ATTR, &out_attrs_[i], sizeof(rknn_tensor_attr));
      if (ret != RKNN_SUCC) throw std::runtime_error("rknn_query output attr failed");
    }
  }

  std::vector<float> infer(const std::vector<const float*>& inputs,
                           const std::vector<size_t>& elem_counts) {
    if (inputs.size() != in_attrs_.size())
      throw std::runtime_error("rknn input count mismatch");
    std::vector<rknn_input> ins(inputs.size());
    std::vector<std::vector<uint8_t>> owned;
    owned.resize(inputs.size());
    for (size_t i = 0; i < inputs.size(); ++i) {
      const auto& attr = in_attrs_[i];
      size_t n = elem_counts[i];
      owned[i].resize(n * sizeof(float));
      std::memcpy(owned[i].data(), inputs[i], n * sizeof(float));
      ins[i].index = static_cast<uint32_t>(i);
      ins[i].buf = owned[i].data();
      ins[i].size = static_cast<uint32_t>(owned[i].size());
      ins[i].pass_through = 0;
      ins[i].type = RKNN_TENSOR_FLOAT32;
      ins[i].fmt = RKNN_TENSOR_UNDEFINED;
    }
    int ret = rknn_inputs_set(ctx_, static_cast<uint32_t>(ins.size()), ins.data());
    if (ret != RKNN_SUCC) throw std::runtime_error("rknn_inputs_set failed");
    ret = rknn_run(ctx_, nullptr);
    if (ret != RKNN_SUCC) throw std::runtime_error("rknn_run failed");
    std::vector<rknn_output> outs(out_attrs_.size());
    for (auto& o : outs) {
      o.want_float = 1;
      o.is_prealloc = 0;
    }
    ret = rknn_outputs_get(ctx_, static_cast<uint32_t>(outs.size()), outs.data(), nullptr);
    if (ret != RKNN_SUCC) throw std::runtime_error("rknn_outputs_get failed");
    if (outs.empty() || !outs[0].buf) throw std::runtime_error("rknn empty output");
    size_t out_elems = outs[0].size / sizeof(float);
    std::vector<float> result(out_elems);
    std::memcpy(result.data(), outs[0].buf, outs[0].size);
    rknn_outputs_release(ctx_, static_cast<uint32_t>(outs.size()), outs.data());
    return result;
  }

 private:
  rknn_context ctx_ = 0;
  std::vector<rknn_tensor_attr> in_attrs_;
  std::vector<rknn_tensor_attr> out_attrs_;

  void destroy() {
    if (ctx_) {
      rknn_destroy(ctx_);
      ctx_ = 0;
    }
    in_attrs_.clear();
    out_attrs_.clear();
  }
};

static Ort::Session make_ort_session(Ort::Env& env, const std::string& path, int intra, int inter) {
  Ort::SessionOptions opts;
  opts.SetIntraOpNumThreads(intra);
  opts.SetInterOpNumThreads(inter);
  opts.SetGraphOptimizationLevel(GraphOptimizationLevel::ORT_ENABLE_ALL);
  opts.DisableMemPattern();
  opts.EnableCpuMemArena();
  return Ort::Session(env, path.c_str(), opts);
}

static std::vector<Ort::Value> run_session_with_all_outputs(
    Ort::Session& sess, const char* const* input_names, Ort::Value* input_values,
    size_t input_count) {
  Ort::AllocatorWithDefaultOptions alloc;
  size_t out_count = sess.GetOutputCount();
  std::vector<std::string> out_name_storage;
  out_name_storage.reserve(out_count);
  std::vector<const char*> out_names;
  out_names.reserve(out_count);
  for (size_t i = 0; i < out_count; ++i) {
    out_name_storage.push_back(sess.GetOutputNameAllocated(i, alloc).get());
    out_names.push_back(out_name_storage.back().c_str());
  }
  return sess.Run(Ort::RunOptions{nullptr}, input_names, input_values, input_count,
                  out_names.data(), out_names.size());
}

static const Ort::Value* find_output_by_name(Ort::Session& sess, Ort::AllocatorWithDefaultOptions& alloc,
                                             const std::vector<Ort::Value>& outputs,
                                             const char* name, size_t fallback) {
  size_t n = sess.GetOutputCount();
  for (size_t i = 0; i < n; ++i) {
    auto out_name = sess.GetOutputNameAllocated(i, alloc);
    if (out_name && std::strcmp(out_name.get(), name) == 0) return &outputs[i];
  }
  return &outputs[fallback];
}

static std::vector<float> tensor_to_float_vector(const Ort::Value& v) {
  auto info = v.GetTensorTypeAndShapeInfo();
  size_t n = info.GetElementCount();
  std::vector<float> out(n);
  const float* p = v.GetTensorData<float>();
  std::memcpy(out.data(), p, n * sizeof(float));
  return out;
}

static std::vector<int16_t> float_to_pcm16(const std::vector<float>& audio) {
  std::vector<int16_t> pcm(audio.size());
  for (size_t i = 0; i < audio.size(); ++i) {
    float v = std::max(-1.0f, std::min(1.0f, audio[i]));
    pcm[i] = static_cast<int16_t>(v * 32767.0f);
  }
  return pcm;
}

static std::vector<float> trim_silence(const std::vector<float>& audio, float threshold = 0.005f,
                                       size_t frame = 512) {
  if (audio.size() < frame) return audio;
  size_t n_frames = audio.size() / frame;
  size_t first = n_frames;
  size_t last = 0;
  for (size_t f = 0; f < n_frames; ++f) {
    double sum = 0;
    for (size_t i = 0; i < frame; ++i) {
      float s = audio[f * frame + i];
      sum += static_cast<double>(s) * s;
    }
    float rms = static_cast<float>(std::sqrt(sum / static_cast<double>(frame)));
    if (rms > threshold) {
      first = std::min(first, f);
      last = std::max(last, f);
    }
  }
  if (first >= n_frames) return audio;
  size_t start = first * frame;
  size_t end = std::min(audio.size(), (last + 1) * frame);
  return std::vector<float>(audio.begin() + static_cast<std::ptrdiff_t>(start),
                            audio.begin() + static_cast<std::ptrdiff_t>(end));
}

struct KokoroEngineImpl {
  Ort::Env ort_env{ORT_LOGGING_LEVEL_WARNING, "kokoro_hybrid"};
  Tokenizer tokenizer;
  std::vector<float> style;
  int seq_len = 32;
  std::unique_ptr<Ort::Session> prefix_sess;
  std::unique_ptr<Ort::Session> tail_rest_sess;
  RknnRunner front_rknn;
  RknnRunner vocoder_front_rknn;
};

static std::vector<float> load_style(const std::string& style_path, const std::string& model_dir,
                                     const std::string& voice) {
  std::vector<std::string> candidates = {style_path};
  if (!voice.empty()) candidates.push_back(resolve_path(model_dir.c_str(), (voice + ".npy").c_str()));
  candidates.push_back(resolve_path(model_dir.c_str(), "style.npy"));
  candidates.push_back(resolve_path(model_dir.c_str(), "default.npy"));
  for (const auto& p : candidates) {
    if (!file_exists(p)) continue;
    auto arr = kokoro_hybrid::load_npy(p);
    if (arr.data.size() < static_cast<size_t>(kStyleDim))
      throw std::runtime_error("style npy too small: " + p);
    std::vector<float> style(kStyleDim);
    std::memcpy(style.data(), arr.data.data(), kStyleDim * sizeof(float));
    return style;
  }
  return std::vector<float>(kStyleDim, 0.0f);
}

}  // namespace

KokoroEngine* kokoro_engine_create(const KokoroEngineConfig* cfg, char* err_buf, size_t err_len) {
  if (!cfg || !cfg->prefix_onnx || !cfg->front_rknn || !cfg->tail_rest_onnx || !cfg->tokens) {
    copy_err(err_buf, err_len, "kokoro_engine_create: missing required paths");
    return nullptr;
  }
  try {
    std::string model_dir = cfg->model_dir ? cfg->model_dir : "";
    std::string prefix = cfg->prefix_onnx;
    std::string front = cfg->front_rknn;
    std::string tail_rest = cfg->tail_rest_onnx;
    std::string vocoder_front = cfg->vocoder_front_rknn ? cfg->vocoder_front_rknn : "";
    std::string tokens = cfg->tokens;
    std::string style_path = cfg->style_npy ? cfg->style_npy : "";
    std::string voice = cfg->voice ? cfg->voice : "default";
    int seq_len = cfg->seq_len > 0 ? cfg->seq_len : 32;

    for (const auto& p : {prefix, front, tail_rest, vocoder_front, tokens}) {
      if (!p.empty() && !file_exists(p))
        throw std::runtime_error("missing kokoro hybrid model: " + p);
    }

    auto eng = std::make_unique<KokoroEngineImpl>();
    eng->seq_len = seq_len;
    eng->tokenizer.load(tokens);
    eng->style = load_style(style_path, model_dir, voice);
    eng->prefix_sess = std::make_unique<Ort::Session>(make_ort_session(eng->ort_env, prefix, 1, 1));
    eng->tail_rest_sess = std::make_unique<Ort::Session>(make_ort_session(eng->ort_env, tail_rest, 4, 1));
    eng->front_rknn.load(front);
    eng->vocoder_front_rknn.load(vocoder_front);
    return reinterpret_cast<KokoroEngine*>(eng.release());
  } catch (const std::exception& e) {
    copy_err(err_buf, err_len, e.what());
    return nullptr;
  } catch (...) {
    copy_err(err_buf, err_len, "kokoro_engine_create: unknown error");
    return nullptr;
  }
}

void kokoro_engine_destroy(KokoroEngine* engine) {
  delete reinterpret_cast<KokoroEngineImpl*>(engine);
}

int kokoro_engine_synthesize_text(KokoroEngine* engine, const char* text, const char* /*voice*/,
                                  float speed, int /*british*/, KokoroPcmCallback callback,
                                  void* user_data, char* err_buf, size_t err_len) {
  auto* impl = reinterpret_cast<KokoroEngineImpl*>(engine);
  if (!impl || !text || !callback) {
    copy_err(err_buf, err_len, "kokoro_engine_synthesize_text: invalid arguments");
    return 1;
  }
  try {
    auto [token_ids, n_tokens] = impl->tokenizer.encode(text, impl->seq_len);
    if (n_tokens <= 0) {
      copy_err(err_buf, err_len, "tokenizer produced zero tokens");
      return 2;
    }

    std::vector<int64_t> tokens_i64 = token_ids;
    std::vector<float> style = impl->style;
    std::vector<float> speed_arr = {speed};

    Ort::MemoryInfo mem = Ort::MemoryInfo::CreateCpu(OrtArenaAllocator, OrtMemTypeDefault);
    std::array<int64_t, 2> token_shape{1, impl->seq_len};
    std::array<int64_t, 2> style_shape{1, kStyleDim};
    std::array<int64_t, 1> speed_shape{1};

    Ort::Value tokens_tensor =
        Ort::Value::CreateTensor(mem, tokens_i64.data(), tokens_i64.size(), token_shape.data(), 2);
    Ort::Value style_tensor =
        Ort::Value::CreateTensor(mem, style.data(), style.size(), style_shape.data(), 2);
    Ort::Value speed_tensor =
        Ort::Value::CreateTensor(mem, speed_arr.data(), speed_arr.size(), speed_shape.data(), 1);

    const char* prefix_in[] = {"tokens", "style", "speed"};
    std::array<Ort::Value, 3> prefix_inputs{std::move(tokens_tensor), std::move(style_tensor),
                                            std::move(speed_tensor)};
    auto prefix_outputs =
        run_session_with_all_outputs(*impl->prefix_sess, prefix_in, prefix_inputs.data(), 3);

    Ort::AllocatorWithDefaultOptions alloc;
    const Ort::Value* decoder_val =
        find_output_by_name(*impl->prefix_sess, alloc, prefix_outputs, kDecoderInput, 0);
    const Ort::Value* style_slice_val =
        find_output_by_name(*impl->prefix_sess, alloc, prefix_outputs, kStyleSlice, 1);
    auto decoder_input = tensor_to_float_vector(*decoder_val);
    auto style_slice = tensor_to_float_vector(*style_slice_val);

    std::vector<const float*> rk_in{decoder_input.data(), style_slice.data()};
    std::vector<size_t> rk_counts{decoder_input.size(), style_slice.size()};
    auto hidden = impl->front_rknn.infer(rk_in, rk_counts);

    rk_in = {hidden.data(), style_slice.data()};
    rk_counts = {hidden.size(), style_slice.size()};
    auto voc_add = impl->vocoder_front_rknn.infer(rk_in, rk_counts);

    std::array<int64_t, 1> tail_speed_shape{1};
    Ort::Value tail_speed =
        Ort::Value::CreateTensor(mem, speed_arr.data(), speed_arr.size(), tail_speed_shape.data(), 1);

    auto make_tail_tensor = [&](const std::vector<float>& data,
                                const std::vector<int64_t>& shape) -> Ort::Value {
      return Ort::Value::CreateTensor<float>(mem, const_cast<float*>(data.data()), data.size(),
                                             shape.data(), shape.size());
    };

    std::vector<int64_t> voc_shape{static_cast<int64_t>(voc_add.size())};
    std::vector<int64_t> hidden_shape{static_cast<int64_t>(hidden.size())};
    std::vector<int64_t> slice_shape{static_cast<int64_t>(style_slice.size())};

    // Re-query actual shapes from ORT output tensors when possible.
    {
      auto ti = decoder_val->GetTensorTypeAndShapeInfo();
      hidden_shape = ti.GetShape();
      ti = style_slice_val->GetTensorTypeAndShapeInfo();
      slice_shape = ti.GetShape();
    }

    std::vector<Ort::Value> tail_inputs;
    tail_inputs.push_back(make_tail_tensor(voc_add, {static_cast<int64_t>(voc_add.size())}));
    tail_inputs.push_back(make_tail_tensor(hidden, hidden_shape));
    tail_inputs.push_back(make_tail_tensor(style_slice, slice_shape));

    // Map by input names on tail_rest session.
    size_t tail_in_count = impl->tail_rest_sess->GetInputCount();
    std::vector<std::string> tail_name_storage;
    std::vector<const char*> tail_names;
    std::vector<Ort::Value> ordered;
    for (size_t i = 0; i < tail_in_count; ++i) {
      auto name = impl->tail_rest_sess->GetInputNameAllocated(i, alloc);
      std::string n = name.get();
      if (n == kVocoderFrontOutput || n.find("Add_5") != std::string::npos)
        ordered.push_back(make_tail_tensor(voc_add, hidden_shape));
      else if (n == kFrontOutput || n.find("Mul_output_0") != std::string::npos)
        ordered.push_back(make_tail_tensor(hidden, hidden_shape));
      else if (n == kStyleSlice || n.find("Slice_2") != std::string::npos)
        ordered.push_back(make_tail_tensor(style_slice, slice_shape));
      else
        ordered.push_back(std::move(tail_speed));
      tail_name_storage.push_back(n);
      tail_names.push_back(tail_name_storage.back().c_str());
    }
    if (ordered.size() != tail_in_count) {
      ordered.clear();
      tail_name_storage.clear();
      tail_names.clear();
      ordered.push_back(make_tail_tensor(voc_add, hidden_shape));
      ordered.push_back(make_tail_tensor(hidden, hidden_shape));
      ordered.push_back(make_tail_tensor(style_slice, slice_shape));
      tail_name_storage = {kVocoderFrontOutput, kFrontOutput, kStyleSlice};
      for (const auto& n : tail_name_storage) tail_names.push_back(n.c_str());
    }

    auto tail_outputs = run_session_with_all_outputs(*impl->tail_rest_sess, tail_names.data(),
                                                     ordered.data(), ordered.size());
    if (tail_outputs.empty()) throw std::runtime_error("tail_rest produced no output");
    auto audio = tensor_to_float_vector(tail_outputs[0]);
    audio = trim_silence(audio);
    auto pcm = float_to_pcm16(audio);
    if (!pcm.empty()) callback(pcm.data(), pcm.size(), user_data);
    return 0;
  } catch (const std::exception& e) {
    copy_err(err_buf, err_len, e.what());
    return 3;
  } catch (...) {
    copy_err(err_buf, err_len, "kokoro_engine_synthesize_text: unknown error");
    return 4;
  }
}
