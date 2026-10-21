// insta360-brygga: Insta360-kamerans förhandsström (Camera SDK, USB) -> H.264
// Annex-B på stdout, i samma form som robotd väntar sig av ffmpeg:
//
//  * en Access Unit Delimiter mellan bilderna (robotd:s AuSplitter delar på AUD),
//  * SPS/PPS före varje nyckelbild (en tittare som ansluter mitt i strömmen
//    kan börja avkoda direkt).
//
// Bilden är två fisheye-bilder sida vid sida (främre + bakre objektivet); de sys
// ihop till en sfär i webbläsaren. Kameran skickar alltid 1664×832 i 30 bilder/s
// på ~8–10 Mbit/s (den bryr sig inte om begärd upplösning/bitrate), för mycket
// för mobilnätet. Med --skala kodas den därför om här: avkoda, skala, x264.
//
// Omkodningen görs med libavcodec direkt och inte med ffmpeg-programmet: ffmpeg
// 4.4 som läser rå H.264 från ett rör håller kvar en hel GOP (1 s med kamerans
// nyckelbild varje sekund) innan bilderna kommer ut, uppmätt 2026-10-21.
//
// Kameran måste stå i USB-läget "Control with Android" (USB-id 2e1a:0002) och
// användaren behöver rättighet till enheten (udev-regel, se bygg.sh).
//
// Användning: insta360-brygga [--skala 1280x640 [--fps 15] [--kbit 1500]]
//             [--kalibrering FIL]
// Avslutas med SIGTERM/SIGINT eller när stdout stängs (robotd dödar oss).

#include <camera/camera.h>
#include <camera/device_discovery.h>
#include <camera/photography_settings.h>
#include <stream/stream_delegate.h>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/opt.h>
#include <libswscale/swscale.h>
}

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdlib>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <sys/stat.h>
#include <unistd.h>
#include <vector>

namespace {

std::atomic<bool> g_sluta{false};
std::atomic<int64_t> g_senaste_bild_ms{0};

int64_t nu_ms() {
    using namespace std::chrono;
    return duration_cast<milliseconds>(steady_clock::now().time_since_epoch()).count();
}

// Skriver allt eller ingenting; false = stdout stängd (robotd borta).
bool skriv_allt(const uint8_t* p, size_t n) {
    while (n > 0) {
        ssize_t w = ::write(STDOUT_FILENO, p, n);
        if (w < 0) {
            if (errno == EINTR) continue;
            return false;
        }
        p += w;
        n -= static_cast<size_t>(w);
    }
    return true;
}

const uint8_t AUD[] = {0, 0, 0, 1, 0x09, 0xf0};

// En bild till robotd. AUD skrivs EFTER bilden (plus en allra först):
// robotd:s AuSplitter släpper en bild när nästa AUD kommer, så med AUD efter
// går bilden iväg direkt i stället för en bild (67 ms vid 15/s) senare.
bool skriv_bild(const uint8_t* d, size_t n) {
    static bool forsta = true;
    if (forsta && !skriv_allt(AUD, sizeof AUD)) return false;
    forsta = false;
    return skriv_allt(d, n) && skriv_allt(AUD, sizeof AUD);
}

struct Nal {
    size_t start; // index för startkoden
    uint8_t typ;
};

std::vector<Nal> hitta_nal(const uint8_t* d, size_t n) {
    std::vector<Nal> ut;
    for (size_t i = 0; i + 3 < n; ++i) {
        if (d[i] == 0 && d[i + 1] == 0) {
            size_t sk = 0;
            if (d[i + 2] == 1) sk = 3;
            else if (d[i + 2] == 0 && d[i + 3] == 1) sk = 4;
            if (sk && i + sk < n) {
                ut.push_back({i, static_cast<uint8_t>(d[i + sk] & 0x1f)});
                i += sk - 1;
            }
        }
    }
    return ut;
}

// Avkodar kamerans bilder, glesar ut till `fps`, skalar och kodar med x264.
// Allt synkront i kamerans anrop: ingen kö = ingen växande fördröjning.
class Omkodare {
public:
    bool starta(int w, int h, int fps, int kbit) {
        fps_ = fps;
        const AVCodec* dk = avcodec_find_decoder(AV_CODEC_ID_H264);
        const AVCodec* ek = avcodec_find_encoder_by_name("libx264");
        if (!dk || !ek) {
            std::cerr << "insta360-brygga: libavcodec saknar H.264-avkodare eller libx264\n";
            return false;
        }
        dek_ = avcodec_alloc_context3(dk);
        dek_->thread_count = 1;              // flera trådar = flera bilders fördröjning
        dek_->flags |= AV_CODEC_FLAG_LOW_DELAY;
        if (avcodec_open2(dek_, dk, nullptr) < 0) return false;

        enk_ = avcodec_alloc_context3(ek);
        enk_->width = w;
        enk_->height = h;
        enk_->pix_fmt = AV_PIX_FMT_YUV420P;
        enk_->time_base = {1, fps};
        enk_->framerate = {fps, 1};
        enk_->bit_rate = int64_t(kbit) * 1000;
        enk_->rc_max_rate = enk_->bit_rate;
        enk_->rc_buffer_size = kbit * 500;   // halv sekund, som ffmpeg-vägen i robotd
        enk_->gop_size = fps;                // nyckelbild varje sekund
        enk_->max_b_frames = 0;
        av_opt_set(enk_->priv_data, "preset", "ultrafast", 0);
        av_opt_set(enk_->priv_data, "tune", "zerolatency", 0);
        if (avcodec_open2(enk_, ek, nullptr) < 0) return false;

        in_ = av_frame_alloc();
        ut_ = av_frame_alloc();
        ut_->format = AV_PIX_FMT_YUV420P;
        ut_->width = w;
        ut_->height = h;
        if (av_frame_get_buffer(ut_, 0) < 0) return false;
        pkt_ = av_packet_alloc();
        return true;
    }

    ~Omkodare() {
        sws_freeContext(sws_);
        av_frame_free(&in_);
        av_frame_free(&ut_);
        av_packet_free(&pkt_);
        avcodec_free_context(&dek_);
        avcodec_free_context(&enk_);
    }

    // En hel bild (Annex-B, utan AUD). false = skrivfel (robotd borta).
    bool bild(const std::vector<uint8_t>& au) {
        int64_t t0 = nu_ms();
        buf_.assign(au.begin(), au.end());
        buf_.resize(au.size() + AV_INPUT_BUFFER_PADDING_SIZE, 0);
        pkt_->data = buf_.data();
        pkt_->size = static_cast<int>(au.size());
        if (avcodec_send_packet(dek_, pkt_) < 0) return true; // trasig bild: hoppa över
        bool ok = true;
        while (ok && avcodec_receive_frame(dek_, in_) == 0) {
            // Gles ut till fps efter klockan (kameran ger 30/s).
            int64_t nu = nu_ms(), steg = 1000 / fps_;
            if (nu < nasta_ms_) continue;
            nasta_ms_ = (nu - nasta_ms_ > steg) ? nu + steg : nasta_ms_ + steg;
            ok = koda(in_);
        }
        tid_ms_ += nu_ms() - t0;
        if (++bilder_ % 300 == 0) {
            std::cerr << "insta360-brygga: omkodning " << tid_ms_ / 300.0 << " ms/bild i snitt\n";
            tid_ms_ = 0;
        }
        return ok;
    }

private:
    bool koda(AVFrame* f) {
        // Kameran ger yuvj420p (full färgomfång, "jpeg"); swscale vill ha
        // yuv420p + uttryckligt omfång. Utbilden är vanligt (begränsat) omfång.
        auto fmt = static_cast<AVPixelFormat>(f->format);
        bool full = fmt == AV_PIX_FMT_YUVJ420P || f->color_range == AVCOL_RANGE_JPEG;
        if (fmt == AV_PIX_FMT_YUVJ420P) fmt = AV_PIX_FMT_YUV420P;
        SwsContext* forra = sws_;
        sws_ = sws_getCachedContext(sws_, f->width, f->height, fmt, ut_->width, ut_->height,
                                    AV_PIX_FMT_YUV420P, SWS_FAST_BILINEAR, nullptr, nullptr, nullptr);
        if (!sws_ || av_frame_make_writable(ut_) < 0) return true;
        if (sws_ != forra) {
            const int* k = sws_getCoefficients(SWS_CS_ITU709);
            sws_setColorspaceDetails(sws_, k, full ? 1 : 0, k, 0, 0, 1 << 16, 1 << 16);
        }
        sws_scale(sws_, f->data, f->linesize, 0, f->height, ut_->data, ut_->linesize);
        ut_->pts = pts_++;
        if (avcodec_send_frame(enk_, ut_) < 0) return true;
        bool ok = true;
        while (ok && avcodec_receive_packet(enk_, pkt_) == 0) {
            ok = skriv_bild(pkt_->data, pkt_->size);
            av_packet_unref(pkt_);
        }
        return ok;
    }

    AVCodecContext* dek_ = nullptr;
    AVCodecContext* enk_ = nullptr;
    SwsContext* sws_ = nullptr;
    AVFrame* in_ = nullptr;
    AVFrame* ut_ = nullptr;
    AVPacket* pkt_ = nullptr;
    std::vector<uint8_t> buf_;
    int fps_ = 15;
    int64_t nasta_ms_ = 0, pts_ = 0, tid_ms_ = 0, bilder_ = 0;
};

class Brygga : public ins_camera::StreamDelegate {
public:
    explicit Brygga(std::unique_ptr<Omkodare> omk) : omk_(std::move(omk)) {}

    void OnVideoData(const uint8_t* data, size_t size, int64_t, uint8_t, int stream_index) override {
        if (stream_index != 0 || g_sluta) return;
        auto nal = hitta_nal(data, size);
        bool har_idr = false, har_sps = false;
        for (size_t i = 0; i < nal.size(); ++i) {
            size_t slut = i + 1 < nal.size() ? nal[i + 1].start : size;
            if (nal[i].typ == 7) { sps_.assign(data + nal[i].start, data + slut); har_sps = true; }
            if (nal[i].typ == 8) pps_.assign(data + nal[i].start, data + slut);
            if (nal[i].typ == 5) har_idr = true;
        }
        if (antal_ < 3) {
            std::cerr << "insta360-brygga: bild " << antal_ << ", " << size << " byte, NAL:";
            for (auto& x : nal) std::cerr << " " << int(x.typ);
            std::cerr << std::endl;
        }
        ++antal_;
        if (!startad_) {
            // Börja först på en nyckelbild med kända SPS/PPS.
            if (!har_idr || sps_.empty() || pps_.empty()) return;
            startad_ = true;
        }
        // Bilden: SPS/PPS före nyckelbilder, och utan kamerans ev. egna AUD
        // (de skulle ge dubbla bilder i robotd).
        au_.clear();
        if (har_idr && !har_sps) {
            au_.insert(au_.end(), sps_.begin(), sps_.end());
            au_.insert(au_.end(), pps_.begin(), pps_.end());
        }
        for (size_t i = 0; i < nal.size(); ++i) {
            if (nal[i].typ == 9) continue;
            size_t slut = i + 1 < nal.size() ? nal[i + 1].start : size;
            au_.insert(au_.end(), data + nal[i].start, data + slut);
        }
        bool ok = omk_ ? omk_->bild(au_) : skriv_bild(au_.data(), au_.size());
        if (!ok) g_sluta = true;
        g_senaste_bild_ms = nu_ms();
    }
    void OnAudioData(const uint8_t*, size_t, int64_t) override {}
    void OnGyroData(const std::vector<ins_camera::GyroData>&) override {}
    void OnExposureData(const ins_camera::ExposureData&) override {}

private:
    std::unique_ptr<Omkodare> omk_;
    std::vector<uint8_t> sps_, pps_, au_;
    bool startad_ = false;
    uint64_t antal_ = 0;
};

} // namespace

int main(int argc, char** argv) {
    int skala_b = 0, skala_h = 0, fps = 15, kbit = 1500;
    std::string kalibrering_fil;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        auto nasta = [&]() -> std::string {
            if (i + 1 >= argc) { std::cerr << a << " saknar värde\n"; std::exit(2); }
            return argv[++i];
        };
        if (a == "--skala") {
            if (std::sscanf(nasta().c_str(), "%dx%d", &skala_b, &skala_h) != 2 || skala_b <= 0 || skala_h <= 0) {
                std::cerr << "--skala ska vara BREDDxHÖJD, t.ex. 1280x640\n";
                return 2;
            }
        } else if (a == "--fps") fps = std::stoi(nasta());
        else if (a == "--kbit") kbit = std::stoi(nasta());
        else if (a == "--kalibrering") kalibrering_fil = nasta();
        else {
            std::cerr << "användning: insta360-brygga [--skala 1280x640 [--fps 15] [--kbit 1500]] "
                         "[--kalibrering FIL]\n";
            return 2;
        }
    }
    if (fps < 1 || fps > 30 || kbit < 100) {
        std::cerr << "--fps 1–30, --kbit minst 100\n";
        return 2;
    }

    std::signal(SIGPIPE, SIG_IGN); // skrivfel hanteras som "robotd borta"
    std::signal(SIGTERM, [](int) { g_sluta = true; });
    std::signal(SIGINT, [](int) { g_sluta = true; });

    av_log_set_level(AV_LOG_ERROR); // x264/swscale pratar annars på varje bild
    std::unique_ptr<Omkodare> omk;
    if (skala_b) {
        omk = std::make_unique<Omkodare>();
        if (!omk->starta(skala_b, skala_h, fps, kbit)) {
            std::cerr << "insta360-brygga: kunde inte starta omkodningen\n";
            return 1;
        }
    }

    if (!kalibrering_fil.empty() && kalibrering_fil[0] != '/') {
        char cwd[4096];
        if (::getcwd(cwd, sizeof cwd)) kalibrering_fil = std::string(cwd) + "/" + kalibrering_fil;
    }
    // SDK:t sparar kamerans inställningstabell i ./jsons; robotd startar oss
    // i en katalog vi kanske inte får skriva i (t.ex. /).
    const char* hem = std::getenv("HOME");
    std::string cache = std::string(hem && *hem ? hem : "/tmp") + "/.cache/insta360-brygga";
    ::mkdir((cache.substr(0, cache.rfind('/'))).c_str(), 0755);
    ::mkdir(cache.c_str(), 0755);
    if (::chdir(cache.c_str()) != 0 && ::chdir("/tmp") != 0) {
        std::cerr << "insta360-brygga: hittar ingen skrivbar katalog\n";
    }

    ins_camera::SetLogLevel(ins_camera::LogLevel::ERR);
    ins_camera::DeviceDiscovery discovery;
    auto lista = discovery.GetAvailableDevices();
    if (lista.empty()) {
        std::cerr << "insta360-brygga: ingen kamera hittad (USB-läge \"Control with Android\"? udev-regel?)\n";
        return 1;
    }
    std::cerr << "insta360-brygga: " << lista[0].camera_name << " serie " << lista[0].serial_number
              << " fw " << lista[0].fw_version << " SDK " << ins_camera::GetSDKVersion() << std::endl;
    auto kamera = std::make_shared<ins_camera::Camera>(lista[0].info);
    if (!kamera->Open()) {
        std::cerr << "insta360-brygga: kunde inte öppna kameran\n";
        return 1;
    }
    discovery.FreeDeviceDescriptors(lista);

    auto pp = kamera->GetPreviewParam();
    if (pp.encode_type != ins_camera::VideoEncodeType::H264) {
        std::cerr << "insta360-brygga: kameran skickar H.265, robotd kräver H.264\n";
        kamera->Close();
        return 1;
    }
    if (!kalibrering_fil.empty()) {
        std::ofstream f(kalibrering_fil);
        for (size_t i = 0; i < pp.GetCalibrationCount(); ++i) f << pp.GetCalibration(i) << "\n";
    }

    std::shared_ptr<ins_camera::StreamDelegate> delegat = std::make_shared<Brygga>(std::move(omk));
    kamera->SetStreamDelegate(delegat);
    // Upplösning och bitrate här ignoreras av X4 Air (alltid 1664×832), men
    // måste vara satta.
    ins_camera::LiveStreamParam param;
    param.video_resolution = ins_camera::VideoResolution::RES_1920_960P30;
    param.lrv_video_resulution = ins_camera::VideoResolution::RES_1920_960P30;
    param.enable_audio = false;
    param.enable_gyro = false;
    param.using_lrv = false;
    if (!kamera->StartLiveStreaming(param)) {
        std::cerr << "insta360-brygga: StartLiveStreaming misslyckades\n";
        kamera->Close();
        return 1;
    }
    std::cerr << "insta360-brygga: ström startad"
              << (skala_b ? " (omkodas till " + std::to_string(skala_b) + "x" + std::to_string(skala_h) + ", " +
                                std::to_string(fps) + " bilder/s, " + std::to_string(kbit) + " kbit/s)"
                          : std::string(" (orörd)"))
              << std::endl;

    // Inga bilder på 5 s -> avsluta med fel så robotd startar om oss.
    g_senaste_bild_ms = nu_ms();
    int kod = 0;
    while (!g_sluta) {
        std::this_thread::sleep_for(std::chrono::milliseconds(200));
        if (nu_ms() - g_senaste_bild_ms > 5000) {
            std::cerr << "insta360-brygga: inga bilder på 5 s\n";
            kod = 1;
            break;
        }
    }
    kamera->StopLiveStreaming();
    kamera->Close();
    return kod;
}
