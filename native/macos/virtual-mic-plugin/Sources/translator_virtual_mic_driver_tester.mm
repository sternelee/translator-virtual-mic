#include <CoreAudio/AudioHardware.h>
#include <CoreFoundation/CoreFoundation.h>
#include <iostream>

#include "translator_virtual_mic_driver.h"

namespace {
void require(bool condition, const char *message) {
    if (!condition) {
        std::cerr << message << std::endl;
        std::exit(1);
    }
}

void require_status(OSStatus status, const char *message) {
    if (status != kAudioHardwareNoError) {
        std::cerr << message << ": " << status << std::endl;
        std::exit(1);
    }
}
}

int main() {
    auto &driver = TranslatorVirtualMicDriver::instance();
    AudioServerPlugInDriverRef driver_ref = reinterpret_cast<AudioServerPlugInDriverRef>(AudioServerPlugIn_Create(nullptr, kAudioServerPlugInTypeUUID));
    require(driver_ref != nullptr, "factory did not return a driver ref");

    void *queried_interface = nullptr;
    require(driver.query_interface(driver_ref, CFUUIDGetUUIDBytes(kAudioServerPlugInDriverInterfaceUUID), &queried_interface) == S_OK, "query interface failed");
    require(queried_interface == driver_ref, "query interface did not return the driver ref");
    std::cout << "factory_driver_ref_ok=1" << std::endl;

    AudioObjectPropertyAddress streams_input = {
        kAudioDevicePropertyStreams,
        kAudioObjectPropertyScopeInput,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress streams_output = {
        kAudioDevicePropertyStreams,
        kAudioObjectPropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress config_input = {
        kAudioDevicePropertyStreamConfiguration,
        kAudioObjectPropertyScopeInput,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress config_output = {
        kAudioDevicePropertyStreamConfiguration,
        kAudioObjectPropertyScopeOutput,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress nominal_rate = {
        kAudioDevicePropertyNominalSampleRate,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress device_control_list = {
        kAudioObjectPropertyControlList,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress plugin_custom_property_info_list = {
        kAudioObjectPropertyCustomPropertyInfoList,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress stream_owned_objects = {
        kAudioObjectPropertyOwnedObjects,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress device_has_changed = {
        kAudioDevicePropertyDeviceHasChanged,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress device_is_running = {
        kAudioDevicePropertyDeviceIsRunning,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress device_is_running_somewhere = {
        kAudioDevicePropertyDeviceIsRunningSomewhere,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress hog_mode = {
        kAudioDevicePropertyHogMode,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress device_can_be_default_system_device = {
        kAudioDevicePropertyDeviceCanBeDefaultSystemDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };
    AudioObjectPropertyAddress translate_uid = {
        kAudioPlugInPropertyTranslateUIDToDevice,
        kAudioObjectPropertyScopeGlobal,
        kAudioObjectPropertyElementMain
    };

    UInt32 size = 0;
    require_status(driver.get_property_data_size(TranslatorVirtualMicDriver::kDeviceObjectID, &streams_input, 0, nullptr, &size), "input streams size");
    std::cout << "input_streams_size=" << size << std::endl;
    require(size == sizeof(AudioObjectID), "unexpected input stream size");

    require_status(driver.get_property_data_size(TranslatorVirtualMicDriver::kDeviceObjectID, &streams_output, 0, nullptr, &size), "output streams size");
    std::cout << "output_streams_size=" << size << std::endl;
    require(size == 0, "unexpected output stream size");

    require_status(driver.get_property_data_size(TranslatorVirtualMicDriver::kPluginObjectID, &plugin_custom_property_info_list, 0, nullptr, &size), "custom property info list size");
    std::cout << "custom_property_info_list_size=" << size << std::endl;
    require(size == 0, "custom property info list should be empty");

    require_status(driver.get_property_data_size(TranslatorVirtualMicDriver::kDeviceObjectID, &device_control_list, 0, nullptr, &size), "device control list size");
    std::cout << "device_control_list_size=" << size << std::endl;
    require(size == 0, "device control list should be empty");

    require_status(driver.get_property_data_size(TranslatorVirtualMicDriver::kStreamObjectID, &stream_owned_objects, 0, nullptr, &size), "stream owned objects size");
    std::cout << "stream_owned_objects_size=" << size << std::endl;
    require(size == 0, "stream owned objects should be empty");

    AudioBufferList input_config = {};
    UInt32 out_size = 0;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &config_input, 0, nullptr, sizeof(input_config), &out_size, &input_config), "input config");
    std::cout << "input_config_channels=" << input_config.mBuffers[0].mNumberChannels << std::endl;
    require(input_config.mBuffers[0].mNumberChannels == 1, "unexpected input channel count");

    AudioBufferList output_config = {};
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &config_output, 0, nullptr, sizeof(output_config), &out_size, &output_config), "output config");
    std::cout << "output_config_channels=" << output_config.mBuffers[0].mNumberChannels << std::endl;
    require(output_config.mBuffers[0].mNumberChannels == 0, "unexpected output channel count");

    CFStringRef known_uid = CFSTR("translator.virtual.mic.device");
    AudioObjectID translated_device = kAudioObjectUnknown;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kPluginObjectID, &translate_uid, sizeof(known_uid), &known_uid, sizeof(translated_device), &out_size, &translated_device), "translate known uid");
    std::cout << "translated_known_uid=" << translated_device << std::endl;
    require(translated_device == TranslatorVirtualMicDriver::kDeviceObjectID, "known uid did not translate");

    CFStringRef unknown_uid = CFSTR("translator.virtual.mic.unknown");
    translated_device = TranslatorVirtualMicDriver::kDeviceObjectID;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kPluginObjectID, &translate_uid, sizeof(unknown_uid), &unknown_uid, sizeof(translated_device), &out_size, &translated_device), "translate unknown uid");
    std::cout << "translated_unknown_uid=" << translated_device << std::endl;
    require(translated_device == kAudioObjectUnknown, "unknown uid should not translate");

    Float64 requested_rate = 48000.0;
    require_status(driver.set_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &nominal_rate, sizeof(requested_rate), &requested_rate), "set supported nominal rate");
    requested_rate = 44100.0;
    const OSStatus unsupported_rate_status = driver.set_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &nominal_rate, sizeof(requested_rate), &requested_rate);
    std::cout << "unsupported_rate_status=" << unsupported_rate_status << std::endl;
    require(unsupported_rate_status != kAudioHardwareNoError, "unsupported nominal rate should fail");

    UInt32 device_has_changed_value = 99;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &device_has_changed, 0, nullptr, sizeof(device_has_changed_value), &out_size, &device_has_changed_value), "device has changed");
    std::cout << "device_has_changed_value=" << device_has_changed_value << std::endl;
    require(device_has_changed_value == 0, "device has changed should be a notification-only placeholder");

    require_status(driver.perform_device_configuration_change(TranslatorVirtualMicDriver::kDeviceObjectID, 0, nullptr), "perform device configuration change");

    UInt32 running_value = 99;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &device_is_running, 0, nullptr, sizeof(running_value), &out_size, &running_value), "device running before start");
    std::cout << "device_is_running_before_start=" << running_value << std::endl;
    require(running_value == 0, "device should not be running before start");

    require_status(driver.start_io(TranslatorVirtualMicDriver::kDeviceObjectID, 1), "start io");
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &device_is_running, 0, nullptr, sizeof(running_value), &out_size, &running_value), "device running after start");
    std::cout << "device_is_running_after_start=" << running_value << std::endl;
    require(running_value == 1, "device should be running after start");

    UInt32 running_somewhere_value = 99;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &device_is_running_somewhere, 0, nullptr, sizeof(running_somewhere_value), &out_size, &running_somewhere_value), "device running somewhere after start");
    std::cout << "device_is_running_somewhere_after_start=" << running_somewhere_value << std::endl;
    require(running_somewhere_value == 1, "device should be running somewhere after start");

    pid_t hog_mode_value = 0;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &hog_mode, 0, nullptr, sizeof(hog_mode_value), &out_size, &hog_mode_value), "hog mode");
    std::cout << "hog_mode=" << hog_mode_value << std::endl;
    require(hog_mode_value == -1, "hog mode should be unowned");

    UInt32 can_be_default_system_device = 99;
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &device_can_be_default_system_device, 0, nullptr, sizeof(can_be_default_system_device), &out_size, &can_be_default_system_device), "device can be default system device");
    std::cout << "device_can_be_default_system_device=" << can_be_default_system_device << std::endl;
    require(can_be_default_system_device == 0, "input-only virtual mic should not be a default system output device");

    require_status(driver.stop_io(TranslatorVirtualMicDriver::kDeviceObjectID, 1), "stop io");
    require_status(driver.get_property_data(TranslatorVirtualMicDriver::kDeviceObjectID, &device_is_running, 0, nullptr, sizeof(running_value), &out_size, &running_value), "device running after stop");
    std::cout << "device_is_running_after_stop=" << running_value << std::endl;
    require(running_value == 0, "device should not be running after stop");

    return 0;
}
