// MudcrabCapture: an SKSE plugin that films reference shots and benchmark flights in Skyrim SE/AE
// (built against CommonLibSSE-NG, GPL-3.0-or-later with the Modding and Linking Exceptions,
// https://github.com/alandtse/CommonLibSSE-NG). Written fresh for Mudcrab from the public API; it
// reads no other mod's code.
//
// What it does
//   F9   play the route CSV as a moving shot: the free camera follows the poses by the real clock,
//        and every frame time is recorded (report in the same shape as the engine's `frame_budget`).
//   F10  stop a run.   F11  stills run: visit each pose, wait `SettleFrames`, save a PNG + JSON.
//   F8   save the current view: a PNG plus an entry in shots.json in the engine's `--shots` format
//        (position, yaw, pitch, hfov), so Mudcrab can render the same composition.
//   [Auto] in the ini starts a run by itself some seconds after a game is loaded, and can quit after.
// Outputs go to <Documents>/My Games/Skyrim Special Edition/SKSE/MudcrabCapture/<run>/.
//
// The pose file is the CSV from tools/capture/route_poses.py: t,x,y,z,yaw_deg,pitch_deg with Skyrim
// units and yaw clockwise from north (the same as `getangle z`).

#include "PCH.h"

#include <Windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <wincodec.h>
#include <wrl/client.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

namespace logger = SKSE::log;
namespace fs = std::filesystem;
using Microsoft::WRL::ComPtr;

namespace
{
	constexpr double kPi = 3.14159265358979323846;
	constexpr double kDegToRad = kPi / 180.0;
	constexpr double kRadToDeg = 180.0 / kPi;

	// ---------------------------------------------------------------- configuration

	struct Pose
	{
		double t{};
		double x{}, y{}, z{};
		double yaw{}, pitch{};  // degrees, Skyrim convention; pitch positive looks UP
		std::string name;       // optional: file stem for the shot (stills)
		double fov{};           // optional: console `fov` value to set before the shot (0 = leave)
		std::string method;     // optional: how the player is moved (setpos | setpos0 | update3d | update3dcam | console); empty = Camera.MoveMethod
	};

	struct Config
	{
		std::string route{ "Data/SKSE/Plugins/MudcrabCapture/route.csv" };
		std::string moveMethod{ "setpos" };  // player mode: setpos | setpos0 | update3d | update3dcam | console (see ApplyPose)
		std::string autoMode{ "none" };  // none | route | stills
		std::string startupCommands;      // run once the game is loaded, ';' separated
		std::string mainMenuCommands;     // run from the main menu (e.g. coc Riverwood), ';' separated
		double      mainMenuDelaySeconds{ 20.0 };
		bool        assumeLoadedAfterMenu{ false };  // coc from the menu sends no load message
		std::string runName{ "run" };
		double      autoDelaySeconds{ 15.0 };
		double      budgetMs{ 6.25 };
		double      freezeHour{ -1.0 };  // game hour to pin, or negative to leave the clock alone
		std::uint32_t weatherFormID{ 0 };  // forced weather, 0 = leave it
		std::uint32_t settleFrames{ 12 };
		double        settleSeconds{ 8.0 };  // eye adaptation and streaming need seconds, not frames
		std::uint32_t stillEvery{ 60 };  // stills: every Nth pose of the route (30 Hz -> 2 s)
		bool          hideUI{ true };
		double        eyeOffset{ 120.5 };  // player mode: camera height above the placed reference (measured: 6000 -> 6120.5)
		bool          usePlayer{ false };  // move the player (first-person camera) instead of the free camera
		bool          quitWhenDone{ false };
		bool          dismissMessageBoxes{ true };  // automated runs: close prompts (e.g. the Survival Mode question)
		double        yawOffsetDeg{ 0.0 };  // added to the CSV yaw (the calibration fixes the rest)
		std::uint32_t hotRoute{ 0x43 };     // F9
		std::uint32_t hotStop{ 0x44 };      // F10
		std::uint32_t hotStills{ 0x57 };    // F11
		std::uint32_t hotPose{ 0x42 };      // F8
	};

	Config                 g_cfg;
	std::vector<Pose>      g_route;
	fs::path               g_pluginDir;  // Data/SKSE/Plugins/MudcrabCapture (resolved at load)

	std::string Trim(std::string s)
	{
		const auto notSpace = [](unsigned char c) { return !std::isspace(c); };
		s.erase(s.begin(), std::find_if(s.begin(), s.end(), notSpace));
		s.erase(std::find_if(s.rbegin(), s.rend(), notSpace).base(), s.end());
		return s;
	}

	void LoadIni(const fs::path& path)
	{
		std::ifstream in(path);
		if (!in) {
			logger::info("no ini at {}, using defaults", path.string());
			return;
		}
		std::map<std::string, std::string> kv;
		std::string                        line, section;
		while (std::getline(in, line)) {
			line = Trim(line);
			if (line.empty() || line[0] == ';' || line[0] == '#') continue;
			if (line.front() == '[') {
				section = line.substr(1, line.find(']') - 1);
				continue;
			}
			if (const auto eq = line.find('='); eq != std::string::npos) {
				kv[section + "." + Trim(line.substr(0, eq))] = Trim(line.substr(eq + 1));
			}
		}
		const auto str = [&](const char* key, std::string& out) {
			if (const auto it = kv.find(key); it != kv.end()) out = it->second;
		};
		const auto num = [&](const char* key, auto& out) {
			if (const auto it = kv.find(key); it != kv.end()) {
				try {
					out = static_cast<std::remove_reference_t<decltype(out)>>(std::stod(it->second));
				} catch (...) {
					logger::warn("ini value {} = '{}' is not a number", key, it->second);
				}
			}
		};
		const auto hex = [&](const char* key, std::uint32_t& out) {
			if (const auto it = kv.find(key); it != kv.end()) {
				try {
					out = static_cast<std::uint32_t>(std::stoul(it->second, nullptr, 0));
				} catch (...) {
					logger::warn("ini value {} = '{}' is not a number", key, it->second);
				}
			}
		};
		str("Route.File", g_cfg.route);
		str("Auto.Mode", g_cfg.autoMode);
		str("Auto.StartupCommands", g_cfg.startupCommands);
		str("Auto.MainMenuCommands", g_cfg.mainMenuCommands);
		num("Auto.MainMenuDelaySeconds", g_cfg.mainMenuDelaySeconds);
		str("Auto.RunName", g_cfg.runName);
		num("Auto.DelaySeconds", g_cfg.autoDelaySeconds);
		num("Benchmark.FrameBudgetMs", g_cfg.budgetMs);
		num("World.FreezeHour", g_cfg.freezeHour);
		hex("World.WeatherFormID", g_cfg.weatherFormID);
		num("Stills.SettleFrames", g_cfg.settleFrames);
		num("Stills.SettleSeconds", g_cfg.settleSeconds);
		num("Stills.EveryNthPose", g_cfg.stillEvery);
		num("Camera.YawOffsetDeg", g_cfg.yawOffsetDeg);
		hex("Keys.Route", g_cfg.hotRoute);
		hex("Keys.Stop", g_cfg.hotStop);
		hex("Keys.Stills", g_cfg.hotStills);
		hex("Keys.SavePose", g_cfg.hotPose);
		num("Camera.EyeOffset", g_cfg.eyeOffset);
		if (const auto it = kv.find("Camera.UsePlayer"); it != kv.end()) g_cfg.usePlayer = it->second == "1";
		if (const auto it = kv.find("Camera.MoveMethod"); it != kv.end() && !it->second.empty()) g_cfg.moveMethod = it->second;
		if (const auto it = kv.find("World.HideUI"); it != kv.end()) g_cfg.hideUI = it->second != "0";
		if (const auto it = kv.find("Auto.QuitWhenDone"); it != kv.end()) g_cfg.quitWhenDone = it->second == "1";
		if (const auto it = kv.find("Auto.DismissMessageBoxes"); it != kv.end()) g_cfg.dismissMessageBoxes = it->second != "0";
		if (const auto it = kv.find("Auto.AssumeLoadedAfterMenu"); it != kv.end()) g_cfg.assumeLoadedAfterMenu = it->second == "1";
	}

	bool LoadRoute(const fs::path& path)
	{
		std::ifstream in(path);
		if (!in) {
			logger::error("route file {} not found", path.string());
			return false;
		}
		std::string line;
		std::getline(in, line);  // header
		g_route.clear();
		while (std::getline(in, line)) {
			if (Trim(line).empty()) continue;
			std::stringstream ss(line);
			std::string       cell;
			std::vector<std::string> cells;
			while (std::getline(ss, cell, ',')) cells.push_back(Trim(cell));
			const auto number = [&](std::size_t i) {
				try {
					return i < cells.size() && !cells[i].empty() ? std::stod(cells[i]) : 0.0;
				} catch (...) {
					return std::nan("");
				}
			};
			if (cells.size() < 6 || std::isnan(number(3))) continue;
			// t,x,y,z,yaw_deg,pitch_deg[,name[,fov[,method]]]
			g_route.push_back({ number(0), number(1), number(2), number(3), number(4), number(5),
				cells.size() > 6 ? cells[6] : std::string{}, cells.size() > 7 ? number(7) : 0.0,
				cells.size() > 8 ? cells[8] : std::string{} });
		}
		logger::info("loaded {} poses ({:.1f} s) from {}", g_route.size(),
			g_route.empty() ? 0.0 : g_route.back().t, path.string());
		return !g_route.empty();
	}

	Pose Sample(double t)
	{
		const auto it = std::lower_bound(g_route.begin(), g_route.end(), t,
			[](const Pose& p, double value) { return p.t < value; });
		if (it == g_route.begin()) return g_route.front();
		if (it == g_route.end()) return g_route.back();
		const Pose& b = *it;
		const Pose& a = *(it - 1);
		const double f = (t - a.t) / std::max(1e-9, b.t - a.t);
		double dyaw = std::fmod(b.yaw - a.yaw + 540.0, 360.0) - 180.0;
		return { t, a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, a.z + (b.z - a.z) * f,
			std::fmod(a.yaw + dyaw * f + 360.0, 360.0), a.pitch + (b.pitch - a.pitch) * f };
	}

	// ---------------------------------------------------------------- game helpers

	void RunConsole(const std::string& command)
	{
		const auto factory = RE::IFormFactory::GetConcreteFormFactoryByType<RE::Script>();
		const auto script = factory ? factory->Create() : nullptr;
		if (!script) {
			logger::error("cannot create a script for '{}'", command);
			return;
		}
		script->SetCommand(command);
		script->CompileAndRun(nullptr);
		delete script;
	}

	RE::FreeCameraState* FreeCamera()
	{
		const auto cam = RE::PlayerCamera::GetSingleton();
		if (!cam) return nullptr;
		return static_cast<RE::FreeCameraState*>(
			cam->GetRuntimeData().cameraStates[RE::CameraStates::kFree].get());
	}

	bool InFreeCamera()
	{
		const auto cam = RE::PlayerCamera::GetSingleton();
		return cam && cam->IsInFreeCameraMode();
	}

	// The camera's own forward vector, read back from the scene graph (the truth, whatever the
	// free-camera state's rotation fields mean): the camera looks along local +Y.
	struct Actual
	{
		bool   ok{};
		double x{}, y{}, z{}, yaw{}, pitch{};
	};

	Actual ReadActual()
	{
		Actual out;
		const auto cam = RE::PlayerCamera::GetSingleton();
		if (!cam || !cam->cameraRoot) return out;
		const auto& w = cam->cameraRoot->world;
		const RE::NiPoint3 forward = w.rotate * RE::NiPoint3{ 0.0f, 1.0f, 0.0f };
		out.ok = true;
		out.x = w.translate.x;
		out.y = w.translate.y;
		out.z = w.translate.z;
		out.yaw = std::fmod(std::atan2(forward.x, forward.y) * kRadToDeg + 360.0, 360.0);
		out.pitch = std::asin(std::clamp<double>(forward.z, -1.0, 1.0)) * kRadToDeg;
		return out;
	}

	// How the free camera's two rotation floats map to yaw and pitch is found by probing at the start
	// of each session, not assumed: write a value, wait, read the real camera back.
	struct Mapping
	{
		bool   done{};
		int    yawField{ 1 };  // which of rotation.x / rotation.y turns the camera left-right
		double yawSign{ 1.0 };
		double yawBase{ 0.0 };   // the actual yaw (degrees) when the yaw field is 0
		double pitchSign{ 1.0 };
		double pitchBase{ 0.0 };
	};
	Mapping g_map;

	void SetField(RE::FreeCameraState* st, int field, float value)
	{
		(field == 0 ? st->rotation.x : st->rotation.y) = value;
	}

	void ApplyPose(const Pose& p, bool first = true)
	{
		if (g_cfg.usePlayer) {
			// The player carries the first-person camera, and the game streams the world and its far LOD around the
			// player (the free camera does not drive streaming). The camera sits `eyeOffset` above the placed
			// reference. Skyrim's X angle is positive looking down; the pose's pitch is positive looking up.
			if (first && p.fov > 0.0) {
				char fovCmd[48];
				std::snprintf(fovCmd, sizeof fovCmd, "fov %.3f", p.fov);
				RunConsole(fovCmd);
			}
			if (const auto player = RE::PlayerCharacter::GetSingleton()) {
				// Measured 2026-10-15: with `setpos` only the first pose of a game session lands; later poses never move
				// the player (identical pictures). The other methods exist to find one that does (docs/process/
				// skyrim-reference-capture.md, "Moving between poses"). Console acts once per pose (`first`).
				const std::string& method = p.method.empty() ? g_cfg.moveMethod : p.method;
				const RE::NiPoint3 pos{ static_cast<float>(p.x), static_cast<float>(p.y), static_cast<float>(p.z - g_cfg.eyeOffset) };
				const double yawDeg = std::fmod(p.yaw + g_cfg.yawOffsetDeg, 360.0);
				const RE::NiPoint3 rot{ static_cast<float>(-p.pitch * kDegToRad), 0.0f, static_cast<float>(yawDeg * kDegToRad) };
				if (method == "console") {
					if (first) {
						char cmd[96];
						std::snprintf(cmd, sizeof cmd, "player.setpos x %.2f", pos.x); RunConsole(cmd);
						std::snprintf(cmd, sizeof cmd, "player.setpos y %.2f", pos.y); RunConsole(cmd);
						std::snprintf(cmd, sizeof cmd, "player.setpos z %.2f", pos.z); RunConsole(cmd);
						std::snprintf(cmd, sizeof cmd, "player.setangle x %.3f", -p.pitch); RunConsole(cmd);
						std::snprintf(cmd, sizeof cmd, "player.setangle z %.3f", yawDeg); RunConsole(cmd);
					}
				} else if (method == "update3d" || method == "update3dcam") {
					// The data position moves but the 3D (and the camera that follows it) may not: warp the 3D to match.
					player->SetPosition(pos, true);
					player->SetAngle(rot);
					player->Update3DPosition(true);
					if (method == "update3dcam") {
						if (const auto cam = RE::PlayerCamera::GetSingleton()) cam->Update();
					}
				} else if (method == "setpos0") {
					player->SetPosition(pos, false);
					player->SetAngle(rot);
				} else {  // setpos: move the character controller too
					player->SetPosition(pos, true);
					player->SetAngle(rot);
				}
			}
			return;
		}
		const auto st = FreeCamera();
		if (!st) return;
		st->translation = { static_cast<float>(p.x), static_cast<float>(p.y), static_cast<float>(p.z) };
		const double yawDeg = p.yaw + g_cfg.yawOffsetDeg;
		const double yawRad = ((yawDeg - g_map.yawBase) * g_map.yawSign) * kDegToRad;
		const double pitchRad = ((p.pitch - g_map.pitchBase) * g_map.pitchSign) * kDegToRad;
		SetField(st, g_map.yawField, static_cast<float>(yawRad));
		SetField(st, 1 - g_map.yawField, static_cast<float>(pitchRad));
	}

	// ---------------------------------------------------------------- run state

	enum class Mode { Idle, Calibrate, Route, Stills };

	struct Run
	{
		Mode   mode{ Mode::Idle };
		Mode   after{ Mode::Idle };  // what to start once calibration finishes
		int    step{};               // calibration step or still index
		int    wait{};               // frames left to wait
		std::chrono::steady_clock::time_point started{};
		std::chrono::steady_clock::time_point lastPresent{};
		std::vector<double> frameMs;
		fs::path outDir;
		std::uint32_t shot{};
		bool uiHidden{};
		bool applied{};  // stills: the current pose has been set and is settling
		std::chrono::steady_clock::time_point settleUntil{};
		bool enteredFreeCam{};
		Actual probe[3]{};
	} g_run;

	std::atomic<bool> g_loaded{ false };
	std::atomic<bool> g_wantPose{ false };
	std::vector<std::string> g_poseEntries;
	fs::path g_poseDir;
	std::uint32_t g_poseCount{ 0 };
	std::chrono::steady_clock::time_point g_loadedAt{};
	bool g_autoDone{ false };
	bool g_startupDone{ false };
	bool g_menuDone{ false };
	std::chrono::steady_clock::time_point g_dataLoadedAt{};

	fs::path DocumentsDir()
	{
		const auto dir = SKSE::log::log_directory();
		return dir ? *dir : fs::path{ "." };
	}

	std::string Stamp()
	{
		const auto now = std::chrono::system_clock::now();
		const auto tt = std::chrono::system_clock::to_time_t(now);
		std::tm tm{};
		localtime_s(&tm, &tt);
		char buf[32];
		std::strftime(buf, sizeof buf, "%Y%m%d-%H%M%S", &tm);
		return buf;
	}

	void PinWorld()
	{
		if (const auto cal = RE::Calendar::GetSingleton(); cal && g_cfg.freezeHour >= 0.0 && cal->gameHour) {
			cal->gameHour->value = static_cast<float>(g_cfg.freezeHour);
			if (cal->timeScale) cal->timeScale->value = 0.0f;
		}
		if (g_cfg.weatherFormID) {
			if (const auto sky = RE::Sky::GetSingleton()) {
				if (const auto weather = RE::TESForm::LookupByID<RE::TESWeather>(g_cfg.weatherFormID)) {
					sky->SetWeather(weather, true, true);  // accelerate: no slow fade from the loading weather
					sky->ForceWeather(weather, true);
				}
			}
		}
	}

	void BeginSession(Mode then)
	{
		g_run = Run{};
		g_run.after = then;
		g_run.outDir = DocumentsDir().parent_path() / "MudcrabCapture" / (g_cfg.runName + "-" + Stamp());
		fs::create_directories(g_run.outDir);
		PinWorld();
		if (g_cfg.hideUI) {
			if (const auto ui = RE::UI::GetSingleton()) {
				ui->ShowMenus(false);
				g_run.uiHidden = true;
			}
		}
		if (!g_cfg.usePlayer && !InFreeCamera()) {
			RunConsole("tfc 1");
			g_run.enteredFreeCam = true;
		}
		g_run.mode = (g_map.done || g_cfg.usePlayer) ? then : Mode::Calibrate;
		g_run.wait = 6;
		g_run.step = 0;
		g_run.started = std::chrono::steady_clock::now();
		logger::info("session started, output {}", g_run.outDir.string());
	}

	void EndSession(const char* why)
	{
		logger::info("run ended: {}", why);
		if (g_run.uiHidden) {
			if (const auto ui = RE::UI::GetSingleton()) ui->ShowMenus(true);
		}
		if (g_run.enteredFreeCam && InFreeCamera()) RunConsole("tfc 1");
		g_run.mode = Mode::Idle;
		// Skyrim crashes in its own exit-time teardown (static destructors of the animation graph, reached from the C runtime's exit,
		// Crash Logger 2026-10-15) after either `qqq` or the main loop's quit. Every file this run writes is closed by now,
		// so the capture ends the process directly and skips that teardown.
		if (g_cfg.quitWhenDone) {
			spdlog::default_logger()->flush();
			TerminateProcess(GetCurrentProcess(), 0);
		}
	}

	// ---------------------------------------------------------------- output

	bool WritePng(const fs::path& path, ID3D11Device* device, ID3D11DeviceContext* ctx, IDXGISwapChain* sc)
	{
		ComPtr<ID3D11Texture2D> back;
		if (FAILED(sc->GetBuffer(0, IID_PPV_ARGS(&back)))) return false;
		D3D11_TEXTURE2D_DESC desc{};
		back->GetDesc(&desc);
		desc.Usage = D3D11_USAGE_STAGING;
		desc.BindFlags = 0;
		desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
		desc.MiscFlags = 0;
		desc.SampleDesc = { 1, 0 };
		ComPtr<ID3D11Texture2D> staging;
		if (FAILED(device->CreateTexture2D(&desc, nullptr, &staging))) return false;
		ctx->CopyResource(staging.Get(), back.Get());
		D3D11_MAPPED_SUBRESOURCE map{};
		if (FAILED(ctx->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &map))) return false;

		std::vector<std::uint8_t> bgr(static_cast<size_t>(desc.Width) * desc.Height * 3);
		bool supported = true;
		for (UINT row = 0; row < desc.Height && supported; ++row) {
			const auto* src = static_cast<const std::uint8_t*>(map.pData) + static_cast<size_t>(row) * map.RowPitch;
			auto*       dst = bgr.data() + static_cast<size_t>(row) * desc.Width * 3;
			for (UINT i = 0; i < desc.Width; ++i) {
				switch (desc.Format) {
				case DXGI_FORMAT_R8G8B8A8_UNORM:
				case DXGI_FORMAT_R8G8B8A8_UNORM_SRGB:
					dst[i * 3 + 0] = src[i * 4 + 2];
					dst[i * 3 + 1] = src[i * 4 + 1];
					dst[i * 3 + 2] = src[i * 4 + 0];
					break;
				case DXGI_FORMAT_B8G8R8A8_UNORM:
				case DXGI_FORMAT_B8G8R8A8_UNORM_SRGB:
					dst[i * 3 + 0] = src[i * 4 + 0];
					dst[i * 3 + 1] = src[i * 4 + 1];
					dst[i * 3 + 2] = src[i * 4 + 2];
					break;
				case DXGI_FORMAT_R10G10B10A2_UNORM: {
					std::uint32_t v;
					std::memcpy(&v, src + i * 4, 4);
					dst[i * 3 + 0] = static_cast<std::uint8_t>(((v >> 20) & 0x3FF) >> 2);
					dst[i * 3 + 1] = static_cast<std::uint8_t>(((v >> 10) & 0x3FF) >> 2);
					dst[i * 3 + 2] = static_cast<std::uint8_t>((v & 0x3FF) >> 2);
					break;
				}
				default:
					supported = false;
					break;
				}
				if (!supported) break;
			}
		}
		ctx->Unmap(staging.Get(), 0);
		if (!supported) {
			logger::error("back buffer format {} is not supported for capture", static_cast<int>(desc.Format));
			return false;
		}

		ComPtr<IWICImagingFactory> factory;
		if (FAILED(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)))) return false;
		ComPtr<IWICStream> stream;
		ComPtr<IWICBitmapEncoder> enc;
		ComPtr<IWICBitmapFrameEncode> frame;
		if (FAILED(factory->CreateStream(&stream)) ||
			FAILED(stream->InitializeFromFilename(path.wstring().c_str(), GENERIC_WRITE)) ||
			FAILED(factory->CreateEncoder(GUID_ContainerFormatPng, nullptr, &enc)) ||
			FAILED(enc->Initialize(stream.Get(), WICBitmapEncoderNoCache)) ||
			FAILED(enc->CreateNewFrame(&frame, nullptr)) || FAILED(frame->Initialize(nullptr)) ||
			FAILED(frame->SetSize(desc.Width, desc.Height))) {
			return false;
		}
		WICPixelFormatGUID fmt = GUID_WICPixelFormat24bppBGR;
		if (FAILED(frame->SetPixelFormat(&fmt)) ||
			FAILED(frame->WritePixels(desc.Height, desc.Width * 3, static_cast<UINT>(bgr.size()), bgr.data())) ||
			FAILED(frame->Commit()) || FAILED(enc->Commit())) {
			return false;
		}
		return true;
	}

	std::string Json(const Pose& want, const Actual& got, double frameMs)
	{
		const auto player = RE::PlayerCharacter::GetSingleton();
		const auto sky = RE::Sky::GetSingleton();
		const auto cal = RE::Calendar::GetSingleton();
		char buf[1536];
		std::snprintf(buf, sizeof buf,
			"{\n  \"shot\": %u,\n  \"route_t\": %.4f,\n"
			"  \"requested\": {\"x\": %.2f, \"y\": %.2f, \"z\": %.2f, \"yaw_deg\": %.2f, \"pitch_deg\": %.2f},\n"
			"  \"actual\": {\"ok\": %s, \"x\": %.2f, \"y\": %.2f, \"z\": %.2f, \"yaw_deg\": %.2f, \"pitch_deg\": %.2f},\n"
			"  \"worldspace\": \"%08X\", \"cell\": \"%08X\",\n"
			"  \"weather\": \"%08X\", \"game_hour\": %.3f, \"frame_ms\": %.3f,\n"
			"  \"runtime\": \"%s\"\n}\n",
			g_run.shot, want.t, want.x, want.y, want.z, want.yaw, want.pitch, got.ok ? "true" : "false", got.x,
			got.y, got.z, got.yaw, got.pitch,
			player && player->GetWorldspace() ? player->GetWorldspace()->GetFormID() : 0u,
			player && player->GetParentCell() ? player->GetParentCell()->GetFormID() : 0u,
			sky && sky->currentWeather ? sky->currentWeather->GetFormID() : 0u,
			cal ? cal->GetHour() : -1.0f, frameMs, REL::Module::get().version().string().c_str());
		return buf;
	}

	std::size_t Rank(const std::vector<double>& sorted, double share)
	{
		// nearest rank: the smallest value with at least `share` of the frames at or below it
		const auto n = sorted.size();
		return std::min(n - 1, static_cast<std::size_t>(std::ceil(share * static_cast<double>(n))) - (n ? 1 : 0));
	}

	void WriteRouteReport()
	{
		auto frames = g_run.frameMs;
		if (frames.size() < 10) {
			logger::warn("route run too short for a report ({} frames)", frames.size());
			return;
		}
		double total = 0.0;
		for (double f : frames) total += f;
		std::sort(frames.begin(), frames.end());
		const double budget = g_cfg.budgetMs;
		const auto over = std::count_if(frames.begin(), frames.end(), [&](double f) { return f > budget; });
		const auto over2 = std::count_if(frames.begin(), frames.end(), [&](double f) { return f > budget * 2.0; });
		const double share = 1.0 - static_cast<double>(over) / static_cast<double>(frames.size());
		std::ofstream out(g_run.outDir / "frame_budget.json");
		out << "{\n  \"budget_ms\": " << budget << ",\n  \"frames\": " << frames.size()
			<< ",\n  \"window_seconds\": " << total / 1000.0 << ",\n  \"p50_ms\": " << frames[Rank(frames, 0.5)]
			<< ",\n  \"p99_ms\": " << frames[Rank(frames, 0.99)] << ",\n  \"p99_9_ms\": " << frames[Rank(frames, 0.999)]
			<< ",\n  \"worst_ms\": " << frames.back() << ",\n  \"frames_over_budget\": " << over
			<< ",\n  \"share_within_budget\": " << share << ",\n  \"frames_over_double_budget\": " << over2
			<< ",\n  \"meets_pass_bar\": " << ((share >= 0.999 && over2 == 0) ? "true" : "false")
			<< ",\n  \"runtime\": \"" << REL::Module::get().version().string() << "\"\n}\n";
		std::ofstream csv(g_run.outDir / "frame_times.csv");
		csv << "frame,ms\n";
		for (std::size_t i = 0; i < g_run.frameMs.size(); ++i) csv << i << "," << g_run.frameMs[i] << "\n";
		logger::info("wrote frame report: {} frames, p99.9 {:.2f} ms, worst {:.2f} ms", frames.size(),
			frames[Rank(frames, 0.999)], frames.back());
	}

	// ---------------------------------------------------------------- per-frame driver

	void SavePose(IDXGISwapChain* sc)
	{
		ComPtr<ID3D11Device> device;
		ComPtr<ID3D11DeviceContext> ctx;
		if (FAILED(sc->GetDevice(IID_PPV_ARGS(&device)))) return;
		device->GetImmediateContext(&ctx);
		if (g_poseDir.empty()) {
			g_poseDir = DocumentsDir().parent_path() / "MudcrabCapture" / ("poses-" + Stamp());
			fs::create_directories(g_poseDir);
		}
		DXGI_SWAP_CHAIN_DESC scDesc{};
		sc->GetDesc(&scDesc);
		const Actual got = ReadActual();
		const auto player = RE::PlayerCharacter::GetSingleton();
		const auto cam = RE::PlayerCamera::GetSingleton();
		char name[48];
		std::snprintf(name, sizeof name, "pose-%03u", g_poseCount++);
		if (!WritePng(g_poseDir / (std::string(name) + ".png"), device.Get(), ctx.Get(), sc)) {
			logger::error("pose capture failed");
			return;
		}
		const auto cell = player ? player->GetParentCell() : nullptr;
		const bool interior = cell && cell->IsInteriorCell();
		const auto world = player ? player->GetWorldspace() : nullptr;
		char entry[1024];
		std::snprintf(entry, sizeof entry,
			"    {\"name\": %s, \"worldspace_id\": %s, \"interior_cell_id\": %s,\n"
			"     \"position\": [%.1f, %.1f, %.1f], \"yaw\": %.2f, \"pitch\": %.2f, \"hfov\": %.2f,\n"
			"     \"reference\": \"%s.png\", \"note\": \"Skyrim world_fov %.2f; ids are form ids; hfov is Skyrim's raw FOV, unverified\"}",
			name, interior || !world ? "null" : std::to_string(world->GetFormID()).c_str(),
			interior ? std::to_string(cell->GetFormID()).c_str() : "null", got.x, got.y, got.z, got.yaw, -got.pitch,
			cam ? cam->GetRuntimeData2().worldFOV : 0.0f, name, cam ? cam->GetRuntimeData2().worldFOV : 0.0f);
		g_poseEntries.emplace_back(entry);
		std::ofstream out(g_poseDir / "shots.json");
		out << "{\n  \"width\": " << scDesc.BufferDesc.Width << ",\n  \"height\": " << scDesc.BufferDesc.Height
			<< ",\n  \"shots\": [\n";
		for (std::size_t i = 0; i < g_poseEntries.size(); ++i) {
			out << g_poseEntries[i] << (i + 1 < g_poseEntries.size() ? ",\n" : "\n");
		}
		out << "  ]\n}\n";
		logger::info("saved pose {} to {}", name, g_poseDir.string());
	}

	// Runs on the render thread right before a frame is presented, with the swap chain in hand.
	void OnPresent(IDXGISwapChain* sc)
	{
		const auto now = std::chrono::steady_clock::now();
		const double frameMs = g_run.lastPresent.time_since_epoch().count() == 0
			? 0.0
			: std::chrono::duration<double, std::milli>(now - g_run.lastPresent).count();
		g_run.lastPresent = now;

		// A new game asks "turn on Survival Mode?" in a message box that nobody is there to answer: close any
		// message box while an automated run is configured.
		static std::uint32_t s_frames = 0;
		if (g_cfg.dismissMessageBoxes && g_cfg.autoMode != "none" && ++s_frames % 20 == 0) {
			SKSE::GetTaskInterface()->AddTask([] {
				const auto ui = RE::UI::GetSingleton();
				const auto queue = RE::UIMessageQueue::GetSingleton();
				if (ui && queue && ui->IsMenuOpen(RE::MessageBoxMenu::MENU_NAME)) {
					logger::info("closing an open message box");
					queue->AddMessage(RE::MessageBoxMenu::MENU_NAME, RE::UI_MESSAGE_TYPE::kHide, nullptr);
					// A hide message does not close it (measured 2026-10-15: the box stayed and paused the game, which froze the
					// camera after the first pose). Press Escape, which a message box takes as its cancel choice ("no").
					static int s_tries = 0;
					if (++s_tries % 3 == 0) {
						keybd_event(VK_ESCAPE, 0x01, KEYEVENTF_SCANCODE, 0);
						Sleep(60);
						keybd_event(VK_ESCAPE, 0x01, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP, 0);
					}
				}
			});
		}
		if (!g_menuDone && g_dataLoadedAt.time_since_epoch().count() != 0 && !g_cfg.mainMenuCommands.empty() &&
			std::chrono::duration<double>(now - g_dataLoadedAt).count() >= g_cfg.mainMenuDelaySeconds) {
			g_menuDone = true;
			std::stringstream menuCmds(g_cfg.mainMenuCommands);
			for (std::string cmd; std::getline(menuCmds, cmd, ';');) {
				if (!Trim(cmd).empty()) SKSE::GetTaskInterface()->AddTask([cmd] { logger::info("menu command: {}", Trim(cmd)); RunConsole(Trim(cmd)); });
			}
			if (g_cfg.assumeLoadedAfterMenu) {
				g_loaded = true;
				g_loadedAt = now;
			}
		}
		if (g_loaded && !g_startupDone &&
			std::chrono::duration<double>(now - g_loadedAt).count() >= g_cfg.autoDelaySeconds) {
			g_startupDone = true;
			std::stringstream ss(g_cfg.startupCommands);
			for (std::string cmd; std::getline(ss, cmd, ';');) {
				if (!Trim(cmd).empty()) SKSE::GetTaskInterface()->AddTask([cmd] { RunConsole(Trim(cmd)); });
			}
		}
		if (g_loaded && !g_autoDone && g_startupDone && g_run.mode == Mode::Idle &&
			std::chrono::duration<double>(now - g_loadedAt).count() >= g_cfg.autoDelaySeconds + 5.0) {
			g_autoDone = true;
			if (g_cfg.autoMode == "route") SKSE::GetTaskInterface()->AddTask([] { BeginSession(Mode::Route); });
			if (g_cfg.autoMode == "stills") SKSE::GetTaskInterface()->AddTask([] { BeginSession(Mode::Stills); });
		}
		if (g_wantPose.exchange(false)) SavePose(sc);
		if (g_run.mode == Mode::Idle || g_route.empty()) return;

		// All game-state work happens on the main thread through the task queue; this thread only
		// counts frames and reads the swap chain.
		switch (g_run.mode) {
		case Mode::Calibrate: {
			if (g_run.wait-- > 0) return;
			const int step = g_run.step++;
			if (step < 3) {
				// step 0 parks the camera at rotation (0,0); step 1 reads that as the base and tries (0.5,0);
				// step 2 reads that and tries (0,0.5)
				SKSE::GetTaskInterface()->AddTask([step] {
					const auto st = FreeCamera();
					if (!st) return;
					st->translation = { static_cast<float>(g_route[0].x), static_cast<float>(g_route[0].y),
						static_cast<float>(g_route[0].z) };
					g_run.probe[step] = ReadActual();
					if (step == 0) st->rotation = { 0.0f, 0.0f };
					else if (step == 1) st->rotation = { 0.5f, 0.0f };
					else st->rotation = { 0.0f, 0.5f };
				});
				g_run.wait = 4;
				return;
			}
			g_run.mode = Mode::Idle;  // the task below sets the real mode once it has the readings
			SKSE::GetTaskInterface()->AddTask([] {
				const Actual base = g_run.probe[1];
				const Actual a = g_run.probe[2];  // after field 0 changed
				const Actual b = ReadActual();    // after field 1 changed
				const auto wrap = [](double d) { return std::fmod(d + 540.0, 360.0) - 180.0; };
				const double dYawA = wrap(a.yaw - base.yaw), dPitchA = a.pitch - base.pitch;
				const double dYawB = wrap(b.yaw - base.yaw), dPitchB = b.pitch - base.pitch;
				if (std::abs(dYawA) > std::abs(dYawB)) {
					g_map.yawField = 0;
					g_map.yawSign = dYawA >= 0 ? 1.0 : -1.0;
					g_map.pitchSign = dPitchB >= 0 ? 1.0 : -1.0;
				} else {
					g_map.yawField = 1;
					g_map.yawSign = dYawB >= 0 ? 1.0 : -1.0;
					g_map.pitchSign = dPitchA >= 0 ? 1.0 : -1.0;
				}
				g_map.yawBase = base.yaw;
				g_map.pitchBase = base.pitch;
				g_map.done = true;
				logger::info("camera mapping: yawField {} yawSign {} pitchSign {} baseYaw {:.1f} basePitch {:.1f} "
							 "(deltas {:.1f}/{:.1f} yaw, {:.1f}/{:.1f} pitch)",
					g_map.yawField, g_map.yawSign, g_map.pitchSign, g_map.yawBase, g_map.pitchBase, dYawA, dYawB,
					dPitchA, dPitchB);
				g_run.mode = g_run.after;
				g_run.wait = 6;
				g_run.step = 0;
				g_run.started = std::chrono::steady_clock::now();
				g_run.frameMs.clear();
			});
			return;
		}
		case Mode::Route: {
			if (g_run.wait > 0) {
				--g_run.wait;
				SKSE::GetTaskInterface()->AddTask([] { ApplyPose(g_route.front()); });
				g_run.started = now;
				return;
			}
			const double t = std::chrono::duration<double>(now - g_run.started).count();
			g_run.frameMs.push_back(frameMs);
			if (t > g_route.back().t) {
				WriteRouteReport();
				SKSE::GetTaskInterface()->AddTask([] { EndSession("route finished"); });
				g_run.mode = Mode::Idle;
				return;
			}
			SKSE::GetTaskInterface()->AddTask([t] { ApplyPose(Sample(t)); });
			return;
		}
		case Mode::Stills: {
			const std::size_t index = static_cast<std::size_t>(g_run.step) * g_cfg.stillEvery;
			if (index >= g_route.size()) {
				SKSE::GetTaskInterface()->AddTask([] { EndSession("stills finished"); });
				g_run.mode = Mode::Idle;
				return;
			}
			const Pose pose = g_route[index];
			// The pose is re-applied every frame while the shot settles: exposure adapts and far cells stream in
			// over seconds after a teleport, and nothing else should move the camera meanwhile.
			const bool firstApply = !g_run.applied;
			SKSE::GetTaskInterface()->AddTask([pose, firstApply] { ApplyPose(pose, firstApply); });
			if (!g_run.applied) {
				g_run.applied = true;
				g_run.wait = static_cast<int>(g_cfg.settleFrames);
				g_run.settleUntil = now + std::chrono::milliseconds(static_cast<long long>(g_cfg.settleSeconds * 1000.0));
				return;
			}
			if (g_run.wait > 0) --g_run.wait;
			{
				static auto s_lastLog = std::chrono::steady_clock::time_point{};
				if (now - s_lastLog > std::chrono::seconds(1)) {
					s_lastLog = now;
					std::string menus;
					if (const auto uiMenus = RE::UI::GetSingleton()) {
						for (const auto& entry : uiMenus->menuMap) {
							if (uiMenus->IsMenuOpen(entry.first)) { menus += entry.first.c_str(); menus += ' '; }
						}
					}
					DWORD foreground = 0;
					GetWindowThreadProcessId(GetForegroundWindow(), &foreground);
					const Actual cam = ReadActual();
					const auto pl = RE::PlayerCharacter::GetSingleton();
					const auto pp = pl ? pl->GetPosition() : RE::NiPoint3{};
					logger::info("settle '{}' method '{}': want {:.0f},{:.0f},{:.0f} player {:.0f},{:.0f},{:.0f} camera {:.0f},{:.0f},{:.0f} yaw {:.1f} player yaw {:.1f} 3D {} focused {} paused {} menus [{}]",
						pose.name, pose.method.empty() ? g_cfg.moveMethod : pose.method, pose.x, pose.y, pose.z, pp.x, pp.y, pp.z,
						cam.x, cam.y, cam.z, cam.yaw, pl ? pl->GetAngleZ() * kRadToDeg : 0.0, pl && pl->Is3DLoaded(), foreground == GetCurrentProcessId(),
							RE::UI::GetSingleton() && RE::UI::GetSingleton()->GameIsPaused(), menus);
				}
			}
			if (g_run.wait > 0 || now < g_run.settleUntil) return;
			ComPtr<ID3D11Device> device;
			ComPtr<ID3D11DeviceContext> ctx;
			if (SUCCEEDED(sc->GetDevice(IID_PPV_ARGS(&device)))) device->GetImmediateContext(&ctx);
			char name[96];
			if (pose.name.empty()) {
				std::snprintf(name, sizeof name, "shot-%04u", g_run.shot);
			} else {
				std::string safe = pose.name.substr(0, 80);
				for (auto& ch : safe) if (!std::isalnum(static_cast<unsigned char>(ch)) && ch != '-' && ch != '_') ch = '_';
				std::snprintf(name, sizeof name, "%s", safe.c_str());
			}
			const auto png = g_run.outDir / (std::string(name) + ".png");
			const Actual got = ReadActual();  // render thread read of a transform: good enough for a log
			if (device && WritePng(png, device.Get(), ctx.Get(), sc)) {
				std::ofstream(g_run.outDir / (std::string(name) + ".json")) << Json(pose, got, frameMs);
				logger::info("saved {}", png.string());
			} else {
				logger::error("could not save {}", png.string());
			}
			++g_run.shot;
			++g_run.step;
			g_run.applied = false;
			return;
		}
		default:
			return;
		}
	}

	// ---------------------------------------------------------------- Present hook

	using PresentFn = HRESULT(STDMETHODCALLTYPE*)(IDXGISwapChain*, UINT, UINT);
	PresentFn g_originalPresent{};

	HRESULT STDMETHODCALLTYPE HookedPresent(IDXGISwapChain* sc, UINT sync, UINT flags)
	{
		try {
			OnPresent(sc);
		} catch (const std::exception& e) {
			logger::error("OnPresent threw: {}", e.what());
			g_run.mode = Mode::Idle;
		}
		return g_originalPresent(sc, sync, flags);
	}

	void InstallPresentHook()
	{
		const auto renderer = RE::BSGraphics::Renderer::GetSingleton();
		if (!renderer) {
			logger::error("no renderer: Present hook not installed");
			return;
		}
		auto* sc = reinterpret_cast<IDXGISwapChain*>(renderer->GetRuntimeData().renderWindows[0].swapChain);
		if (!sc) {
			logger::error("no swap chain: Present hook not installed");
			return;
		}
		void** vtable = *reinterpret_cast<void***>(sc);
		DWORD  old{};
		VirtualProtect(&vtable[8], sizeof(void*), PAGE_EXECUTE_READWRITE, &old);
		g_originalPresent = reinterpret_cast<PresentFn>(vtable[8]);
		vtable[8] = reinterpret_cast<void*>(&HookedPresent);
		VirtualProtect(&vtable[8], sizeof(void*), old, &old);
		logger::info("Present hook installed");
	}

	// ---------------------------------------------------------------- input

	class KeySink : public RE::BSTEventSink<RE::InputEvent*>
	{
	public:
		RE::BSEventNotifyControl ProcessEvent(RE::InputEvent* const* a_event, RE::BSTEventSource<RE::InputEvent*>*) override
		{
			if (!a_event) return RE::BSEventNotifyControl::kContinue;
			for (auto e = *a_event; e; e = e->next) {
				const auto button = e->AsButtonEvent();
				if (!button || !button->IsDown() || e->GetDevice() != RE::INPUT_DEVICE::kKeyboard) continue;
				const auto key = button->GetIDCode();
				if (key == g_cfg.hotRoute && g_run.mode == Mode::Idle) BeginSession(Mode::Route);
				else if (key == g_cfg.hotStills && g_run.mode == Mode::Idle) BeginSession(Mode::Stills);
				else if (key == g_cfg.hotStop && g_run.mode != Mode::Idle) EndSession("stopped by key");
				else if (key == g_cfg.hotPose) g_wantPose = true;
			}
			return RE::BSEventNotifyControl::kContinue;
		}
	};
	KeySink g_keys;

	void OnMessage(SKSE::MessagingInterface::Message* msg)
	{
		switch (msg->type) {
		case SKSE::MessagingInterface::kDataLoaded:
			CoInitializeEx(nullptr, COINIT_MULTITHREADED);
			InstallPresentHook();
			g_dataLoadedAt = std::chrono::steady_clock::now();
			if (const auto input = RE::BSInputDeviceManager::GetSingleton()) input->AddEventSink(&g_keys);
			break;
		case SKSE::MessagingInterface::kPostLoadGame:
		case SKSE::MessagingInterface::kNewGame:
			g_loaded = true;
			g_loadedAt = std::chrono::steady_clock::now();
			g_startupDone = g_autoDone = false;
			break;
		default:
			break;
		}
	}
}

SKSEPluginLoad(const SKSE::LoadInterface* skse)
{
	SKSE::Init(skse);
	const auto log = SKSE::log::log_directory();
	if (log) {
		auto sink = std::make_shared<spdlog::sinks::basic_file_sink_mt>((*log / "MudcrabCapture.log").string(), true);
		auto logger_ = std::make_shared<spdlog::logger>("capture", sink);
		spdlog::set_default_logger(logger_);
		spdlog::set_level(spdlog::level::info);
		spdlog::flush_on(spdlog::level::info);
	}
	logger::info("MudcrabCapture 0.1.0 on runtime {}", REL::Module::get().version().string());
	g_pluginDir = fs::path{ "Data/SKSE/Plugins/MudcrabCapture" };
	LoadIni(g_pluginDir / "MudcrabCapture.ini");
	LoadRoute(g_cfg.route);
	SKSE::GetMessagingInterface()->RegisterListener(OnMessage);
	return true;
}
