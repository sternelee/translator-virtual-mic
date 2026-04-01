#include "translator_virtual_mic_driver.h"

#include <CoreFoundation/CoreFoundation.h>
#include <CoreAudio/AudioHardware.h>
#include <mach/mach_time.h>
#include <stddef.h>
#include <string.h>

namespace {
AudioServerPlugInDriverInterface kDriverInterface = {};
constexpr Float64 kNominalSampleRate = 48000.0;
constexpr UInt32 kChannelCount = 1;
constexpr UInt32 kZeroTimeStampPeriod = 480;
constexpr const char *kDeviceName = "Translator Virtual Mic";
constexpr const char *kManufacturerName = "Translator Virtual Mic";
constexpr const char *kStreamName = "Translator Virtual Mic Stream";
constexpr const char *kDeviceUID = "translator.virtual.mic.device";
constexpr const char *kModelUID = "translator.virtual.mic.model";
constexpr const char *kResourceBundleName = "Resources";

CFStringRef copy_cf_string(const char *value) {
    return CFStringCreateWithCString(kCFAllocatorDefault, value, kCFStringEncodingUTF8);
}

bool addresses_match_selector(const AudioObjectPropertyAddress *address, AudioObjectPropertySelector selector) {
    return address != nullptr && address->mSelector == selector;
}

bool uuid_bytes_equal(REFIID lhs, CFUUIDRef rhs) {
    const CFUUIDBytes rhs_bytes = CFUUIDGetUUIDBytes(rhs);
    return memcmp(&lhs, &rhs_bytes, sizeof(CFUUIDBytes)) == 0;
}

constexpr UInt32 single_buffer_list_size() {
    return static_cast<UInt32>(offsetof(AudioBufferList, mBuffers) + sizeof(AudioBuffer));
}

AudioStreamBasicDescription make_mono_float_format() {
    AudioStreamBasicDescription asbd = {};
    asbd.mSampleRate = kNominalSampleRate;
    asbd.mFormatID = kAudioFormatLinearPCM;
    asbd.mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked;
    asbd.mBytesPerPacket = sizeof(Float32);
    asbd.mFramesPerPacket = 1;
    asbd.mBytesPerFrame = sizeof(Float32);
    asbd.mChannelsPerFrame = kChannelCount;
    asbd.mBitsPerChannel = 32;
    return asbd;
}

bool is_input_scope(const AudioObjectPropertyAddress *address) {
    return address != nullptr &&
        (address->mScope == kAudioObjectPropertyScopeInput || address->mScope == kAudioObjectPropertyScopeGlobal);
}

bool is_output_scope(const AudioObjectPropertyAddress *address) {
    return address != nullptr && address->mScope == kAudioObjectPropertyScopeOutput;
}
} // namespace

TranslatorVirtualMicDriver &TranslatorVirtualMicDriver::instance() {
    static TranslatorVirtualMicDriver driver;
    return driver;
}

AudioServerPlugInDriverInterface *TranslatorVirtualMicDriver::driver_interface() {
    static bool initialized = false;
    if (!initialized) {
        kDriverInterface._reserved = nullptr;
        kDriverInterface.QueryInterface = [](void *in_driver, REFIID in_uuid, LPVOID *out_interface) {
            return TranslatorVirtualMicDriver::instance().query_interface(in_uuid, out_interface);
        };
        kDriverInterface.AddRef = [](void *in_driver) {
            return TranslatorVirtualMicDriver::instance().add_ref();
        };
        kDriverInterface.Release = [](void *in_driver) {
            return TranslatorVirtualMicDriver::instance().release();
        };
        kDriverInterface.Initialize = [](AudioServerPlugInDriverRef in_driver, AudioServerPlugInHostRef in_host) {
            return TranslatorVirtualMicDriver::instance().initialize(in_host);
        };
        kDriverInterface.CreateDevice = [](AudioServerPlugInDriverRef in_driver, CFDictionaryRef in_description, const AudioServerPlugInClientInfo *in_client_info, AudioObjectID *out_device_object_id) {
            return TranslatorVirtualMicDriver::instance().create_device(out_device_object_id);
        };
        kDriverInterface.DestroyDevice = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id) {
            return TranslatorVirtualMicDriver::instance().destroy_device(in_device_object_id);
        };
        kDriverInterface.AddDeviceClient = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, const AudioServerPlugInClientInfo *in_client_info) {
            return TranslatorVirtualMicDriver::instance().add_device_client(in_device_object_id, in_client_info);
        };
        kDriverInterface.RemoveDeviceClient = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, const AudioServerPlugInClientInfo *in_client_info) {
            return TranslatorVirtualMicDriver::instance().remove_device_client(in_device_object_id, in_client_info);
        };
        kDriverInterface.PerformDeviceConfigurationChange = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt64 in_change_action, void *in_change_info) {
            return TranslatorVirtualMicDriver::instance().perform_device_configuration_change(in_device_object_id, in_change_action, in_change_info);
        };
        kDriverInterface.AbortDeviceConfigurationChange = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt64 in_change_action, void *in_change_info) {
            return TranslatorVirtualMicDriver::instance().abort_device_configuration_change(in_device_object_id, in_change_action, in_change_info);
        };
        kDriverInterface.HasProperty = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_object_id, pid_t in_client_process_id, const AudioObjectPropertyAddress *in_address) {
            return TranslatorVirtualMicDriver::instance().has_property(in_object_id, in_address);
        };
        kDriverInterface.IsPropertySettable = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_object_id, pid_t in_client_process_id, const AudioObjectPropertyAddress *in_address, Boolean *out_is_settable) {
            return TranslatorVirtualMicDriver::instance().is_property_settable(in_object_id, in_address, out_is_settable);
        };
        kDriverInterface.GetPropertyDataSize = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_object_id, pid_t in_client_process_id, const AudioObjectPropertyAddress *in_address, UInt32 in_qualifier_data_size, const void *in_qualifier_data, UInt32 *out_data_size) {
            return TranslatorVirtualMicDriver::instance().get_property_data_size(in_object_id, in_address, in_qualifier_data_size, in_qualifier_data, out_data_size);
        };
        kDriverInterface.GetPropertyData = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_object_id, pid_t in_client_process_id, const AudioObjectPropertyAddress *in_address, UInt32 in_qualifier_data_size, const void *in_qualifier_data, UInt32 in_data_size, UInt32 *out_data_size, void *out_data) {
            return TranslatorVirtualMicDriver::instance().get_property_data(in_object_id, in_address, in_qualifier_data_size, in_qualifier_data, in_data_size, out_data_size, out_data);
        };
        kDriverInterface.SetPropertyData = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_object_id, pid_t in_client_process_id, const AudioObjectPropertyAddress *in_address, UInt32 in_qualifier_data_size, const void *in_qualifier_data, UInt32 in_data_size, const void *in_data) {
            return TranslatorVirtualMicDriver::instance().set_property_data(in_object_id, in_address, in_data_size, in_data);
        };
        kDriverInterface.StartIO = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt32 in_client_id) {
            return TranslatorVirtualMicDriver::instance().start_io(in_device_object_id, in_client_id);
        };
        kDriverInterface.StopIO = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt32 in_client_id) {
            return TranslatorVirtualMicDriver::instance().stop_io(in_device_object_id, in_client_id);
        };
        kDriverInterface.GetZeroTimeStamp = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt32 in_client_id, Float64 *out_sample_time, UInt64 *out_host_time, UInt64 *out_seed) {
            return TranslatorVirtualMicDriver::instance().get_zero_time_stamp(in_device_object_id, out_sample_time, out_host_time, out_seed);
        };
        kDriverInterface.WillDoIOOperation = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt32 in_client_id, UInt32 in_operation_id, Boolean *out_will_do, Boolean *out_will_do_in_place) {
            return TranslatorVirtualMicDriver::instance().will_do_io_operation(in_device_object_id, in_operation_id, out_will_do, out_will_do_in_place);
        };
        kDriverInterface.BeginIOOperation = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt32 in_client_id, UInt32 in_operation_id, UInt32 in_io_buffer_frame_size, const AudioServerPlugInIOCycleInfo *in_io_cycle_info) {
            return TranslatorVirtualMicDriver::instance().begin_io_operation(in_device_object_id, in_operation_id, in_io_buffer_frame_size, in_io_cycle_info);
        };
        kDriverInterface.DoIOOperation = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, AudioObjectID in_stream_object_id, UInt32 in_client_id, UInt32 in_operation_id, UInt32 in_io_buffer_frame_size, const AudioServerPlugInIOCycleInfo *in_io_cycle_info, void *io_main_buffer, void *io_secondary_buffer) {
            return TranslatorVirtualMicDriver::instance().do_io_operation(in_device_object_id, in_stream_object_id, in_operation_id, in_io_buffer_frame_size, io_main_buffer, io_secondary_buffer);
        };
        kDriverInterface.EndIOOperation = [](AudioServerPlugInDriverRef in_driver, AudioObjectID in_device_object_id, UInt32 in_client_id, UInt32 in_operation_id, UInt32 in_io_buffer_frame_size, const AudioServerPlugInIOCycleInfo *in_io_cycle_info) {
            return TranslatorVirtualMicDriver::instance().end_io_operation(in_device_object_id, in_operation_id, in_io_buffer_frame_size, in_io_cycle_info);
        };
        initialized = true;
    }
    return &kDriverInterface;
}

TranslatorVirtualMicDriver::TranslatorVirtualMicDriver()
    : ref_count_(1),
      host_(nullptr),
      zero_timestamp_seed_(1),
      sample_time_(0),
      active_io_clients_(0),
      render_source_(static_cast<uint32_t>(kNominalSampleRate), kChannelCount) {}

HRESULT TranslatorVirtualMicDriver::query_interface(REFIID uuid, LPVOID *out_interface) {
    if (out_interface == nullptr) {
        return E_POINTER;
    }
    *out_interface = nullptr;
    const CFUUIDRef driver_uuid = kAudioServerPlugInDriverInterfaceUUID;
    const CFUUIDRef unknown_uuid = IUnknownUUID;
    if (uuid_bytes_equal(uuid, driver_uuid) || uuid_bytes_equal(uuid, unknown_uuid)) {
        *out_interface = driver_interface();
        add_ref();
        return S_OK;
    }
    return E_NOINTERFACE;
}

ULONG TranslatorVirtualMicDriver::add_ref() {
    return ++ref_count_;
}

ULONG TranslatorVirtualMicDriver::release() {
    const ULONG count = --ref_count_;
    if (count == 0) {
        ref_count_ = 1;
        return 0;
    }
    return count;
}

OSStatus TranslatorVirtualMicDriver::initialize(AudioServerPlugInHostRef host) {
    host_ = host;
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::create_device(AudioObjectID *out_device_object_id) const {
    if (out_device_object_id == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    *out_device_object_id = kDeviceObjectID;
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::destroy_device(AudioObjectID device_object_id) const {
    return device_object_id == kDeviceObjectID ? kAudioHardwareNoError : kAudioHardwareBadObjectError;
}

OSStatus TranslatorVirtualMicDriver::add_device_client(AudioObjectID device_object_id, const AudioServerPlugInClientInfo *client_info) const {
    return device_object_id == kDeviceObjectID ? kAudioHardwareNoError : kAudioHardwareBadObjectError;
}

OSStatus TranslatorVirtualMicDriver::remove_device_client(AudioObjectID device_object_id, const AudioServerPlugInClientInfo *client_info) const {
    return device_object_id == kDeviceObjectID ? kAudioHardwareNoError : kAudioHardwareBadObjectError;
}

OSStatus TranslatorVirtualMicDriver::perform_device_configuration_change(AudioObjectID device_object_id, UInt64 change_action, void *change_info) const {
    if (device_object_id != kDeviceObjectID) {
        return kAudioHardwareBadObjectError;
    }
    notify_device_configuration_changed();
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::abort_device_configuration_change(AudioObjectID device_object_id, UInt64 change_action, void *change_info) const {
    return device_object_id == kDeviceObjectID ? kAudioHardwareNoError : kAudioHardwareBadObjectError;
}

Boolean TranslatorVirtualMicDriver::has_property(AudioObjectID object_id, const AudioObjectPropertyAddress *address) const {
    if (!is_known_object(object_id) || address == nullptr) {
        return false;
    }

    switch (object_id) {
        case kPluginObjectID:
            return addresses_match_selector(address, kAudioObjectPropertyBaseClass) ||
                addresses_match_selector(address, kAudioObjectPropertyClass) ||
                addresses_match_selector(address, kAudioObjectPropertyOwner) ||
                addresses_match_selector(address, kAudioObjectPropertyManufacturer) ||
                addresses_match_selector(address, kAudioObjectPropertyOwnedObjects) ||
                addresses_match_selector(address, kAudioObjectPropertyName) ||
                addresses_match_selector(address, kAudioPlugInPropertyDeviceList) ||
                addresses_match_selector(address, kAudioPlugInPropertyTranslateUIDToDevice) ||
                addresses_match_selector(address, kAudioPlugInPropertyResourceBundle);
        case kDeviceObjectID:
            return addresses_match_selector(address, kAudioObjectPropertyBaseClass) ||
                addresses_match_selector(address, kAudioObjectPropertyClass) ||
                addresses_match_selector(address, kAudioObjectPropertyOwner) ||
                addresses_match_selector(address, kAudioObjectPropertyName) ||
                addresses_match_selector(address, kAudioObjectPropertyManufacturer) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceUID) ||
                addresses_match_selector(address, kAudioDevicePropertyModelUID) ||
                addresses_match_selector(address, kAudioDevicePropertyTransportType) ||
                addresses_match_selector(address, kAudioObjectPropertyOwnedObjects) ||
                addresses_match_selector(address, kAudioDevicePropertyStreams) ||
                addresses_match_selector(address, kAudioDevicePropertyLatency) ||
                addresses_match_selector(address, kAudioDevicePropertySafetyOffset) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceHasChanged) ||
                addresses_match_selector(address, kAudioDevicePropertyStreamConfiguration) ||
                addresses_match_selector(address, kAudioDevicePropertyNominalSampleRate) ||
                addresses_match_selector(address, kAudioDevicePropertyAvailableNominalSampleRates) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceIsAlive) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceIsRunning) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceIsRunningSomewhere) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceCanBeDefaultDevice) ||
                addresses_match_selector(address, kAudioDevicePropertyDeviceCanBeDefaultSystemDevice) ||
                addresses_match_selector(address, kAudioDevicePropertyZeroTimeStampPeriod) ||
                addresses_match_selector(address, kAudioDevicePropertyClockIsStable) ||
                addresses_match_selector(address, kAudioDevicePropertyHogMode) ||
                addresses_match_selector(address, kAudioObjectPropertyControlList);
        case kStreamObjectID:
            return addresses_match_selector(address, kAudioObjectPropertyBaseClass) ||
                addresses_match_selector(address, kAudioObjectPropertyClass) ||
                addresses_match_selector(address, kAudioObjectPropertyOwner) ||
                addresses_match_selector(address, kAudioObjectPropertyName) ||
                addresses_match_selector(address, kAudioStreamPropertyDirection) ||
                addresses_match_selector(address, kAudioStreamPropertyStartingChannel) ||
                addresses_match_selector(address, kAudioStreamPropertyLatency) ||
                addresses_match_selector(address, kAudioStreamPropertyTerminalType) ||
                addresses_match_selector(address, kAudioStreamPropertyVirtualFormat) ||
                addresses_match_selector(address, kAudioStreamPropertyAvailableVirtualFormats) ||
                addresses_match_selector(address, kAudioStreamPropertyPhysicalFormat) ||
                addresses_match_selector(address, kAudioObjectPropertyOwnedObjects);
        default:
            return false;
    }
}

OSStatus TranslatorVirtualMicDriver::is_property_settable(AudioObjectID object_id, const AudioObjectPropertyAddress *address, Boolean *out_is_settable) const {
    if (out_is_settable == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    if (!has_property(object_id, address)) {
        return kAudioHardwareUnknownPropertyError;
    }
    *out_is_settable = object_id == kDeviceObjectID && addresses_match_selector(address, kAudioDevicePropertyNominalSampleRate);
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::get_property_data_size(AudioObjectID object_id, const AudioObjectPropertyAddress *address, UInt32 qualifier_data_size, const void *qualifier_data, UInt32 *out_data_size) const {
    if (out_data_size == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    if (!has_property(object_id, address)) {
        return kAudioHardwareUnknownPropertyError;
    }

    switch (address->mSelector) {
        case kAudioPlugInPropertyDeviceList:
            *out_data_size = sizeof(AudioObjectID);
            break;
        case kAudioObjectPropertyOwnedObjects:
            if (object_id == kPluginObjectID || object_id == kDeviceObjectID) {
                *out_data_size = sizeof(AudioObjectID);
            } else {
                *out_data_size = 0;
            }
            break;
        case kAudioDevicePropertyStreams:
            *out_data_size = is_output_scope(address) ? 0 : sizeof(AudioObjectID);
            break;
        case kAudioObjectPropertyControlList:
            *out_data_size = 0;
            break;
        case kAudioObjectPropertyBaseClass:
        case kAudioObjectPropertyClass:
            *out_data_size = sizeof(AudioClassID);
            break;
        case kAudioObjectPropertyOwner:
            *out_data_size = sizeof(AudioObjectID);
            break;
        case kAudioPlugInPropertyTranslateUIDToDevice:
            if (qualifier_data_size != sizeof(CFStringRef) || qualifier_data == nullptr) {
                return kAudioHardwareIllegalOperationError;
            }
            *out_data_size = sizeof(AudioObjectID);
            break;
        case kAudioDevicePropertyAvailableNominalSampleRates:
            *out_data_size = sizeof(AudioValueRange);
            break;
        case kAudioStreamPropertyAvailableVirtualFormats:
            *out_data_size = sizeof(AudioStreamRangedDescription);
            break;
        case kAudioDevicePropertyNominalSampleRate:
            *out_data_size = sizeof(Float64);
            break;
        case kAudioDevicePropertyStreamConfiguration:
            *out_data_size = single_buffer_list_size();
            break;
        case kAudioDevicePropertyZeroTimeStampPeriod:
        case kAudioDevicePropertyTransportType:
        case kAudioDevicePropertyLatency:
        case kAudioDevicePropertySafetyOffset:
        case kAudioDevicePropertyDeviceHasChanged:
        case kAudioStreamPropertyStartingChannel:
            *out_data_size = sizeof(UInt32);
            break;
        case kAudioDevicePropertyClockIsStable:
        case kAudioDevicePropertyDeviceIsAlive:
        case kAudioDevicePropertyDeviceIsRunning:
        case kAudioDevicePropertyDeviceIsRunningSomewhere:
        case kAudioDevicePropertyDeviceCanBeDefaultDevice:
        case kAudioDevicePropertyDeviceCanBeDefaultSystemDevice:
        case kAudioStreamPropertyDirection:
        case kAudioStreamPropertyTerminalType:
            *out_data_size = sizeof(UInt32);
            break;
        case kAudioDevicePropertyHogMode:
            *out_data_size = sizeof(pid_t);
            break;
        case kAudioObjectPropertyName:
        case kAudioObjectPropertyManufacturer:
        case kAudioDevicePropertyDeviceUID:
        case kAudioDevicePropertyModelUID:
        case kAudioPlugInPropertyResourceBundle:
            *out_data_size = sizeof(CFStringRef);
            break;
        case kAudioStreamPropertyVirtualFormat:
        case kAudioStreamPropertyPhysicalFormat:
            *out_data_size = sizeof(AudioStreamBasicDescription);
            break;
        default:
            return kAudioHardwareUnknownPropertyError;
    }
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::get_property_data(AudioObjectID object_id, const AudioObjectPropertyAddress *address, UInt32 qualifier_data_size, const void *qualifier_data, UInt32 in_data_size, UInt32 *out_data_size, void *out_data) const {
    if (out_data_size == nullptr || out_data == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    UInt32 required_size = 0;
    OSStatus status = get_property_data_size(object_id, address, qualifier_data_size, qualifier_data, &required_size);
    if (status != kAudioHardwareNoError) {
        return status;
    }
    if (in_data_size < required_size) {
        return kAudioHardwareBadPropertySizeError;
    }

    switch (address->mSelector) {
        case kAudioObjectPropertyBaseClass: {
            auto *class_id = reinterpret_cast<AudioClassID *>(out_data);
            *class_id = kAudioObjectClassID;
            break;
        }
        case kAudioObjectPropertyClass: {
            auto *class_id = reinterpret_cast<AudioClassID *>(out_data);
            if (object_id == kPluginObjectID) {
                *class_id = kAudioPlugInClassID;
            } else if (object_id == kDeviceObjectID) {
                *class_id = kAudioDeviceClassID;
            } else {
                *class_id = kAudioStreamClassID;
            }
            break;
        }
        case kAudioObjectPropertyOwner: {
            auto *owner = reinterpret_cast<AudioObjectID *>(out_data);
            if (object_id == kPluginObjectID) {
                *owner = kAudioObjectUnknown;
            } else if (object_id == kDeviceObjectID) {
                *owner = kPluginObjectID;
            } else {
                *owner = kDeviceObjectID;
            }
            break;
        }
        case kAudioPlugInPropertyDeviceList: {
            *reinterpret_cast<AudioObjectID *>(out_data) = kDeviceObjectID;
            break;
        }
        case kAudioObjectPropertyOwnedObjects: {
            if (required_size > 0) {
                *reinterpret_cast<AudioObjectID *>(out_data) = (object_id == kPluginObjectID) ? kDeviceObjectID : kStreamObjectID;
            }
            break;
        }
        case kAudioDevicePropertyStreams: {
            if (required_size > 0) {
                *reinterpret_cast<AudioObjectID *>(out_data) = kStreamObjectID;
            }
            break;
        }
        case kAudioObjectPropertyControlList: {
            break;
        }
        case kAudioDevicePropertyNominalSampleRate: {
            *reinterpret_cast<Float64 *>(out_data) = kNominalSampleRate;
            break;
        }
        case kAudioDevicePropertyTransportType: {
            *reinterpret_cast<UInt32 *>(out_data) = kAudioDeviceTransportTypeVirtual;
            break;
        }
        case kAudioDevicePropertyAvailableNominalSampleRates: {
            auto *range = reinterpret_cast<AudioValueRange *>(out_data);
            range->mMinimum = kNominalSampleRate;
            range->mMaximum = kNominalSampleRate;
            break;
        }
        case kAudioDevicePropertyStreamConfiguration: {
            auto *buffer_list = reinterpret_cast<AudioBufferList *>(out_data);
            buffer_list->mNumberBuffers = 1;
            buffer_list->mBuffers[0].mNumberChannels = is_output_scope(address) ? 0 : kChannelCount;
            buffer_list->mBuffers[0].mDataByteSize = 0;
            buffer_list->mBuffers[0].mData = nullptr;
            break;
        }
        case kAudioDevicePropertyZeroTimeStampPeriod: {
            *reinterpret_cast<UInt32 *>(out_data) = kZeroTimeStampPeriod;
            break;
        }
        case kAudioDevicePropertyLatency:
        case kAudioDevicePropertySafetyOffset: {
            *reinterpret_cast<UInt32 *>(out_data) = 0;
            break;
        }
        case kAudioDevicePropertyDeviceHasChanged: {
            *reinterpret_cast<UInt32 *>(out_data) = 0;
            break;
        }
        case kAudioDevicePropertyClockIsStable:
        case kAudioDevicePropertyDeviceIsAlive:
        case kAudioDevicePropertyDeviceIsRunning:
        case kAudioDevicePropertyDeviceIsRunningSomewhere:
        case kAudioDevicePropertyDeviceCanBeDefaultDevice:
        case kAudioDevicePropertyDeviceCanBeDefaultSystemDevice: {
            UInt32 value = 1;
            if (address->mSelector == kAudioDevicePropertyDeviceIsRunning ||
                address->mSelector == kAudioDevicePropertyDeviceIsRunningSomewhere) {
                value = is_running() ? 1U : 0U;
            }
            *reinterpret_cast<UInt32 *>(out_data) = value;
            break;
        }
        case kAudioDevicePropertyHogMode: {
            *reinterpret_cast<pid_t *>(out_data) = -1;
            break;
        }
        case kAudioStreamPropertyDirection: {
            *reinterpret_cast<UInt32 *>(out_data) = 1;
            break;
        }
        case kAudioStreamPropertyStartingChannel: {
            *reinterpret_cast<UInt32 *>(out_data) = 1;
            break;
        }
        case kAudioStreamPropertyTerminalType: {
            *reinterpret_cast<UInt32 *>(out_data) = kAudioStreamTerminalTypeMicrophone;
            break;
        }
        case kAudioStreamPropertyVirtualFormat: {
            *reinterpret_cast<AudioStreamBasicDescription *>(out_data) = make_mono_float_format();
            break;
        }
        case kAudioStreamPropertyPhysicalFormat: {
            *reinterpret_cast<AudioStreamBasicDescription *>(out_data) = make_mono_float_format();
            break;
        }
        case kAudioStreamPropertyAvailableVirtualFormats: {
            auto *desc = reinterpret_cast<AudioStreamRangedDescription *>(out_data);
            memset(desc, 0, sizeof(AudioStreamRangedDescription));
            desc->mFormat = make_mono_float_format();
            desc->mSampleRateRange.mMinimum = kNominalSampleRate;
            desc->mSampleRateRange.mMaximum = kNominalSampleRate;
            break;
        }
        case kAudioPlugInPropertyTranslateUIDToDevice: {
            const CFStringRef uid = *reinterpret_cast<const CFStringRef *>(qualifier_data);
            auto *translated_device_id = reinterpret_cast<AudioObjectID *>(out_data);
            if (uid != nullptr && CFStringCompare(uid, CFSTR("translator.virtual.mic.device"), 0) == kCFCompareEqualTo) {
                *translated_device_id = kDeviceObjectID;
            } else {
                *translated_device_id = kAudioObjectUnknown;
            }
            break;
        }
        case kAudioObjectPropertyName: {
            *reinterpret_cast<CFStringRef *>(out_data) = copy_cf_string(object_id == kStreamObjectID ? kStreamName : kDeviceName);
            break;
        }
        case kAudioObjectPropertyManufacturer: {
            *reinterpret_cast<CFStringRef *>(out_data) = copy_cf_string(kManufacturerName);
            break;
        }
        case kAudioDevicePropertyDeviceUID: {
            *reinterpret_cast<CFStringRef *>(out_data) = copy_cf_string(kDeviceUID);
            break;
        }
        case kAudioDevicePropertyModelUID: {
            *reinterpret_cast<CFStringRef *>(out_data) = copy_cf_string(kModelUID);
            break;
        }
        case kAudioPlugInPropertyResourceBundle: {
            *reinterpret_cast<CFStringRef *>(out_data) = copy_cf_string(kResourceBundleName);
            break;
        }
        default:
            return kAudioHardwareUnknownPropertyError;
    }

    *out_data_size = required_size;
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::set_property_data(AudioObjectID object_id, const AudioObjectPropertyAddress *address, UInt32 in_data_size, const void *in_data) const {
    if (address == nullptr || in_data == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    if (object_id != kDeviceObjectID) {
        return kAudioHardwareBadObjectError;
    }
    if (address->mSelector == kAudioDevicePropertyNominalSampleRate) {
        if (in_data_size != sizeof(Float64)) {
            return kAudioHardwareBadPropertySizeError;
        }
        const Float64 requested_rate = *reinterpret_cast<const Float64 *>(in_data);
        return requested_rate == kNominalSampleRate ? kAudioHardwareNoError : kAudioHardwareIllegalOperationError;
    }
    return kAudioHardwareUnsupportedOperationError;
}

OSStatus TranslatorVirtualMicDriver::start_io(AudioObjectID device_object_id, UInt32 client_id) {
    if (device_object_id != kDeviceObjectID) {
        return kAudioHardwareBadObjectError;
    }
    const bool was_running = is_running();
    ++active_io_clients_;
    if (!was_running && is_running()) {
        notify_io_state_changed();
    }
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::stop_io(AudioObjectID device_object_id, UInt32 client_id) {
    if (device_object_id != kDeviceObjectID) {
        return kAudioHardwareBadObjectError;
    }
    const bool was_running = is_running();
    if (active_io_clients_ > 0) {
        --active_io_clients_;
    }
    if (was_running && !is_running()) {
        notify_io_state_changed();
    }
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::get_zero_time_stamp(AudioObjectID device_object_id, Float64 *out_sample_time, UInt64 *out_host_time, UInt64 *out_seed) const {
    if (device_object_id != kDeviceObjectID || out_sample_time == nullptr || out_host_time == nullptr || out_seed == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    *out_sample_time = sample_time_.load();
    *out_host_time = mach_absolute_time();
    *out_seed = zero_timestamp_seed_.load();
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::will_do_io_operation(AudioObjectID device_object_id, UInt32 operation_id, Boolean *out_will_do, Boolean *out_will_do_in_place) const {
    if (device_object_id != kDeviceObjectID || out_will_do == nullptr || out_will_do_in_place == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }
    *out_will_do = (operation_id == kAudioServerPlugInIOOperationReadInput);
    *out_will_do_in_place = true;
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::begin_io_operation(AudioObjectID device_object_id, UInt32 operation_id, UInt32 io_buffer_frame_size, const AudioServerPlugInIOCycleInfo *io_cycle_info) const {
    return device_object_id == kDeviceObjectID ? kAudioHardwareNoError : kAudioHardwareBadObjectError;
}

OSStatus TranslatorVirtualMicDriver::do_io_operation(AudioObjectID device_object_id, AudioObjectID stream_object_id, UInt32 operation_id, UInt32 io_buffer_frame_size, void *io_main_buffer, void *io_secondary_buffer) {
    if (device_object_id != kDeviceObjectID || stream_object_id != kStreamObjectID) {
        return kAudioHardwareBadObjectError;
    }
    if (operation_id != kAudioServerPlugInIOOperationReadInput) {
        return kAudioHardwareNoError;
    }

    auto *buffer = reinterpret_cast<Float32 *>(io_main_buffer != nullptr ? io_main_buffer : io_secondary_buffer);
    if (buffer == nullptr) {
        return kAudioHardwareIllegalOperationError;
    }

    const TranslatorVirtualMicRenderResult result = render_source_.render(buffer, io_buffer_frame_size);
    sample_time_.store(sample_time_.load() + static_cast<Float64>(result.frames_produced));
    zero_timestamp_seed_.store(result.timestamp_ns == 0 ? zero_timestamp_seed_.load() : result.timestamp_ns);
    return kAudioHardwareNoError;
}

OSStatus TranslatorVirtualMicDriver::end_io_operation(AudioObjectID device_object_id, UInt32 operation_id, UInt32 io_buffer_frame_size, const AudioServerPlugInIOCycleInfo *io_cycle_info) const {
    return device_object_id == kDeviceObjectID ? kAudioHardwareNoError : kAudioHardwareBadObjectError;
}

bool TranslatorVirtualMicDriver::is_known_object(AudioObjectID object_id) const {
    return object_id == kPluginObjectID || object_id == kDeviceObjectID || object_id == kStreamObjectID;
}

UInt32 TranslatorVirtualMicDriver::io_client_count() const {
    return active_io_clients_.load();
}

bool TranslatorVirtualMicDriver::is_running() const {
    return io_client_count() > 0;
}

OSStatus TranslatorVirtualMicDriver::notify_properties_changed(AudioObjectID object_id, UInt32 number_addresses, const AudioObjectPropertyAddress *addresses) const {
    if (host_ == nullptr || number_addresses == 0 || addresses == nullptr || host_->PropertiesChanged == nullptr) {
        return kAudioHardwareNoError;
    }
    return host_->PropertiesChanged(host_, object_id, number_addresses, addresses);
}

void TranslatorVirtualMicDriver::notify_io_state_changed() const {
    const AudioObjectPropertyAddress addresses[] = {
        {kAudioDevicePropertyDeviceIsRunning, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain},
        {kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain},
    };
    static_cast<void>(notify_properties_changed(kDeviceObjectID, 2, addresses));
}

void TranslatorVirtualMicDriver::notify_device_configuration_changed() const {
    const AudioObjectPropertyAddress addresses[] = {
        {kAudioDevicePropertyDeviceHasChanged, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain},
        {kAudioObjectPropertyOwnedObjects, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain},
        {kAudioDevicePropertyStreams, kAudioObjectPropertyScopeInput, kAudioObjectPropertyElementMain},
        {kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain},
        {kAudioDevicePropertyStreamConfiguration, kAudioObjectPropertyScopeInput, kAudioObjectPropertyElementMain},
    };
    static_cast<void>(notify_properties_changed(kDeviceObjectID, 5, addresses));
}

extern "C" void *TranslatorVirtualMicFactory(CFAllocatorRef allocator, CFUUIDRef type_uuid) {
    if (type_uuid == nullptr || !CFEqual(type_uuid, kAudioServerPlugInTypeUUID)) {
        return nullptr;
    }
    return TranslatorVirtualMicDriver::driver_interface();
}
