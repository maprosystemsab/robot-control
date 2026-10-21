// insta360-brygga: Insta360-kamerans förhandsström (Camera SDK, USB) -> H.264
// Annex-B på stdout, i samma form som robotd väntar sig av ffmpeg:
//
//  * en Access Unit Delimiter före varje bild (robotd:s AuSplitter delar på AUD),
//  * SPS/PPS före varje nyckelbild (en tittare som ansluter mitt i strömmen
//    kan börja avkoda direkt).
//
// Bilden är två fisheye-bilder sida vid sida (främre + bakre objektivet); de sys
// ihop till en sfär i webbläsaren, så här görs ingen omkodning alls.
//
// Kameran måste stå i USB-läget "Control with Android" (USB-id 2e1a:0002) och
// användaren behöver rättighet till enheten (udev-regel, se bygg.sh).
//
// Användning: insta360-brygga [--upplosning 1440x720] [--kbit 2000] [--lrv]
//             [--kalibrering FIL]
// Avslutas med SIGTERM/SIGINT eller när stdout stängs (robotd dödar oss).

#include <camera/camera.h>
#include <camera/device_discovery.h>
#include <camera/photography_settings.h>
#include <stream/stream_delegate.h>

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <iostream>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
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

struct Nal {
    size_t start; // index för startkoden
    size_t data;  // index för NAL-huvudet
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
                ut.push_back({i, i + sk, static_cast<uint8_t>(d[i + sk] & 0x1f)});
                i += sk - 1;
            }
        }
    }
    return ut;
}

class Brygga : public ins_camera::StreamDelegate {
public:
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
        if (antal_ < 5) {
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
        static const uint8_t aud[] = {0, 0, 0, 1, 0x09, 0xf0};
        bool ok = skriv_allt(aud, sizeof aud);
        if (ok && har_idr && !har_sps) {
            ok = skriv_allt(sps_.data(), sps_.size()) && skriv_allt(pps_.data(), pps_.size());
        }
        // AUD som kameran ev. redan skickar skulle ge dubbla bilder i robotd.
        for (size_t i = 0; ok && i < nal.size(); ++i) {
            if (nal[i].typ == 9) continue;
            size_t slut = i + 1 < nal.size() ? nal[i + 1].start : size;
            ok = skriv_allt(data + nal[i].start, slut - nal[i].start);
        }
        if (!ok) g_sluta = true;
        g_senaste_bild_ms = nu_ms();
    }
    void OnAudioData(const uint8_t*, size_t, int64_t) override {}
    void OnGyroData(const std::vector<ins_camera::GyroData>&) override {}
    void OnExposureData(const ins_camera::ExposureData&) override {}

private:
    std::vector<uint8_t> sps_, pps_;
    bool startad_ = false;
    uint64_t antal_ = 0;
};

bool las_upplosning(const std::string& s, ins_camera::VideoResolution& ut) {
    using R = ins_camera::VideoResolution;
    if (s == "1920x960") ut = R::RES_1920_960P30;
    else if (s == "1440x720") ut = R::RES_1440_720P30;
    else if (s == "960x480") ut = R::RES_960_480P30;
    else return false;
    return true;
}

} // namespace

int main(int argc, char** argv) {
    ins_camera::VideoResolution upplosning = ins_camera::VideoResolution::RES_1440_720P30;
    uint32_t kbit = 2000;
    bool lrv = false;
    std::string kalibrering_fil;
    for (int i = 1; i < argc; ++i) {
        std::string a = argv[i];
        auto nasta = [&]() -> std::string {
            if (i + 1 >= argc) { std::cerr << a << " saknar värde\n"; std::exit(2); }
            return argv[++i];
        };
        if (a == "--upplosning") {
            if (!las_upplosning(nasta(), upplosning)) {
                std::cerr << "okänd upplösning (1920x960, 1440x720, 960x480)\n";
                return 2;
            }
        } else if (a == "--kbit") kbit = static_cast<uint32_t>(std::stoul(nasta()));
        else if (a == "--lrv") lrv = true;
        else if (a == "--kalibrering") kalibrering_fil = nasta();
        else {
            std::cerr << "användning: insta360-brygga [--upplosning 1440x720] [--kbit 2000] [--lrv] "
                         "[--kalibrering FIL]\n";
            return 2;
        }
    }

    std::signal(SIGPIPE, SIG_IGN); // skrivfel hanteras som "robotd borta"
    std::signal(SIGTERM, [](int) { g_sluta = true; });
    std::signal(SIGINT, [](int) { g_sluta = true; });

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

    std::shared_ptr<ins_camera::StreamDelegate> delegat = std::make_shared<Brygga>();
    kamera->SetStreamDelegate(delegat);
    ins_camera::LiveStreamParam param;
    param.video_resolution = upplosning;
    param.lrv_video_resulution = upplosning;
    param.video_bitrate = kbit * 1000;
    param.lrv_video_bitrate = kbit * 1000;
    param.enable_audio = false;
    param.enable_gyro = false;
    param.using_lrv = lrv;
    if (!kamera->StartLiveStreaming(param)) {
        std::cerr << "insta360-brygga: StartLiveStreaming misslyckades\n";
        kamera->Close();
        return 1;
    }
    std::cerr << "insta360-brygga: ström startad\n";

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
