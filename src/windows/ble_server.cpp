// win_ble_server.cpp

#include <windows.h>
#include <string>
#include <thread>
#include <mutex>
#include <atomic>

#include <winrt/Windows.Devices.Bluetooth.GenericAttributeProfile.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Storage.Streams.h>
#include <winrt/Windows.Security.Cryptography.h>

using namespace winrt;
using namespace Windows::Foundation;
using namespace Windows::Devices::Bluetooth::GenericAttributeProfile;
using namespace Windows::Storage::Streams;

// State
std::thread ble_thread;
std::atomic<bool> running{false};
GattServiceProvider g_service_provider = nullptr;

// C-compatible callback type
extern "C" typedef void(__stdcall *DataCallback)(const uint8_t* data, size_t len);
DataCallback g_callback = nullptr;

// Convert string UUID to GUID
winrt::guid parse_guid(const char* uuid_cstr) {
    std::wstring wuuid(strlen(uuid_cstr), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, uuid_cstr, -1, wuuid.data(), static_cast<int>(wuuid.size()));
    return winrt::guid{wuuid.c_str()};
}

extern "C" __declspec(dllexport)
void start_ble_server(const char* name,
                      const char* service_uuid,
                      const char* characteristic_uuid,
                      DataCallback callback) {
    if (running) return;
    running = true;
    g_callback = callback;

    ble_thread = std::thread([=] {
        init_apartment();

        try {
            auto service_guid = parse_guid(service_uuid);
            auto char_guid = parse_guid(characteristic_uuid);

            auto result = GattServiceProvider::CreateAsync(service_guid).get();
            if (result.Error() != BluetoothError::Success) {
                OutputDebugStringW(L"Failed to create GATT service\n");
                return;
            }

            g_service_provider = result.ServiceProvider();

            GattLocalCharacteristicParameters charParams;
            charParams.CharacteristicProperties(GattCharacteristicProperties::Write);
            charParams.WriteProtectionLevel(GattProtectionLevel::Plain);
            charParams.UserDescription(L"Command Receiver");

            auto char_result = g_service_provider.Service().CreateCharacteristicAsync(char_guid, charParams).get();
            if (char_result.Error() != BluetoothError::Success) {
                OutputDebugStringW(L"Failed to create characteristic\n");
                return;
            }

            auto characteristic = char_result.Characteristic();

            characteristic.WriteRequested([](auto&&, auto&& args) {
                auto deferral = args.GetDeferral();
                auto request = args.GetRequestAsync().get();
                auto buffer = request.Value();
                auto reader = DataReader::FromBuffer(buffer);

                std::vector<uint8_t> data;
                while (reader.UnconsumedBufferLength() > 0) {
                    data.push_back(reader.ReadByte());
                }
                if (g_callback) {
                    g_callback(data.data(), data.size());
                }

                request.Respond();
                deferral.Complete();
            });

            GattServiceProviderAdvertisingParameters advParams;
            advParams.IsConnectable(true);
            advParams.IsDiscoverable(true);

            g_service_provider.StartAdvertising(advParams);
            OutputDebugStringA("BLE GATT server started\n");

            while (running) {
                std::this_thread::sleep_for(std::chrono::milliseconds(200));
            }

            g_service_provider.StopAdvertising();
            g_service_provider = nullptr;
            OutputDebugStringA("BLE GATT server stopped\n");

        } catch (const std::exception& e) {
            OutputDebugStringA(("Exception: " + std::string(e.what()) + "\n").c_str());
        }
    });
}

extern "C" __declspec(dllexport)
void stop_ble_server() {
    if (!running) return;
    running = false;

    if (ble_thread.joinable()) {
        ble_thread.join();
    }
    g_callback = nullptr;
}
