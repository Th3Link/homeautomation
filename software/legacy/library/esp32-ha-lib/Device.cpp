#include <nvs_flash.h>
#include <esp_mac.h>
#include <esp_log.h>
#include <esp_ota_ops.h>
#include "Device.hpp"
#include <chrono>
#include <cstring>
#include <algorithm>

const char* Device::TAG = "Device";

Device::Device(ICAN& ic): m_can(ic)
{
    m_can.add_dispatcher(this);
}

void Device::init()
{
    
}

bool Device::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    static bool uid_selected = false;
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::REQUEST_PARAMETER:
        {
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::APPLICATION_VERSION_STRING), data, data_len, request);
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::DEVICE_UID0), data, data_len, request);
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::DEVICE_UID1), data, data_len, request);
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::CUSTOM_STRING), data, data_len, request);
            
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::UPTIME), data, data_len, request);
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::BAUDRATE), data, data_len, request);
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::HW_REV), data, data_len, request);
            Device::dispatch((identifier & 0xFFFFFF00) + static_cast<uint8_t>(ICAN::MSG_ID_t::SENSOR_LEGACY_MODE), data, data_len, request);
            return false;
        }
        case ICAN::MSG_ID_t::DEVICE_GROUP:
        {
            if (request)
            {
                uint8_t data[1] {0};
                m_can.send(ICAN::MSG_ID_t::DEVICE_GROUP, data, sizeof(data), false);
            }
            return true;
        }

        case ICAN::MSG_ID_t::APPLICATION_VERSION_STRING:
        {
            if (request)
            {
                const esp_app_desc_t* desc = esp_ota_get_app_description();
                const char release_prefix[] = "release/";
                const size_t rpl = sizeof(release_prefix)-1;
                
                uint8_t data[8] {0};
                size_t len = strlen(desc->version);
                int min_len = std::min(static_cast<int>(len),8);
                
                if (strncmp(desc->version, release_prefix, rpl) == 0)
                {
                    min_len = std::min(static_cast<int>(len-rpl),8);
                    for (unsigned int i = 0; i < min_len; i++)
                    {
                        data[i] = desc->version[i+rpl];
                    }
                }
                else
                {
                    for (unsigned int i = 0; i < min_len; i++)
                    {
                        data[i] = desc->version[i];
                    }
                }
                m_can.send(ICAN::MSG_ID_t::APPLICATION_VERSION_STRING, data, min_len, false);
            }
            return true;
        }

        case ICAN::MSG_ID_t::DEVICE_ID_TYPE:
        {
            if (request)
            {
                uint8_t id_type[2] {0};
                id_type[0] = m_can.get_id();
                id_type[1] = m_can.get_type();
                m_can.send(ICAN::MSG_ID_t::DEVICE_ID_TYPE, id_type, sizeof(id_type), false);
            }
            else if (uid_selected && data_len == 2)
            {
                nvs_handle_t nvs_handle;
                nvs_open("storage", NVS_READWRITE, &nvs_handle);

                nvs_set_u8(nvs_handle, "can_type", data[1]);
                if (data[0] != 0)
                {
                    nvs_set_u8(nvs_handle, "can_id", data[0]);
                }
                nvs_commit(nvs_handle);
                nvs_close(nvs_handle);
            }
            return true;
        }
        
        case ICAN::MSG_ID_t::DEVICE_UID0:
            //fall through
        case ICAN::MSG_ID_t::DEVICE_UID1:
        {
            uint8_t chipid[8] {0};
            esp_efuse_mac_get_default(chipid);
            if (request)
            {
                m_can.send(static_cast<ICAN::MSG_ID_t>(identifier & 0xFF), chipid, 
                    sizeof(chipid), false);
            }
            else
            {
                if (data_len == 8)
                {
                    uid_selected = true;
                    bool all_zero = true;
                    for (unsigned int i = 0; i < 8; i++)
                    {
                        if (chipid[i] != data[i])
                        {
                            uid_selected = false;
                        }
                        if (data[i])
                        {
                            all_zero = false;
                        }
                    }
                    if (all_zero)
                    {
                        uid_selected = true;
                    }
                }
            }
            return true;
        }
        case ICAN::MSG_ID_t::BAUDRATE:
        {
            nvs_handle_t nvs_handle;
            nvs_open("storage", NVS_READWRITE, &nvs_handle);
            if (request)
            {
                // custom string
                uint8_t data_out[1] {0};

                nvs_get_u8(nvs_handle, "can_bitrate", &data_out[0]);

                m_can.send(ICAN::MSG_ID_t::BAUDRATE, data_out, 
                    1, false);
            }
            else
            {
                if ((data_len == 1) && (data[0] > 0))
                {
                    nvs_set_u8(nvs_handle, "can_bitrate", data[0]);
                    nvs_commit(nvs_handle);
                }
            }
            nvs_close(nvs_handle);
            return true;
        }
        case ICAN::MSG_ID_t::CUSTOM_STRING:
        {
            nvs_handle_t nvs_handle;
            nvs_open("storage", NVS_READWRITE, &nvs_handle);
            if (request)
            {
                // custom string
                uint8_t custom_string[9] {0};
                size_t custom_string_len = 9;

                nvs_get_str(nvs_handle, "custom_string", reinterpret_cast<char*>(&custom_string[0]), &custom_string_len);

                m_can.send(ICAN::MSG_ID_t::CUSTOM_STRING, custom_string, 
                    std::min(static_cast<size_t>(8), custom_string_len), false);
            }
            else
            {
                uint8_t custom_string[9] {0};
                for (unsigned int i = 0; i < data_len; i++)
                {
                    custom_string[i] = data[i];
                }
                nvs_set_str(nvs_handle, "custom_string", reinterpret_cast<char*>(&custom_string[0]));
                nvs_commit(nvs_handle);
            }
            
            nvs_close(nvs_handle);
            return true;
        }
        case ICAN::MSG_ID_t::UPTIME:
        {
            if (request)
            {
                // uptime
                union {
                    unsigned int uptime;
                    uint8_t uptime8[4];
                };
                
                auto uptime_auto = std::chrono::duration_cast<std::chrono::minutes>(
                      std::chrono::system_clock::now().time_since_epoch()).count();
                
                uptime = static_cast<decltype(uptime)>(uptime_auto);

                m_can.send(ICAN::MSG_ID_t::UPTIME, uptime8, 
                    sizeof(uptime8), false);
            }
            return true;
        }
        case ICAN::MSG_ID_t::HW_REV:
        {
            if (data_len == 1 && !request)
            {
                nvs_handle_t nvs_handle;
                nvs_open("storage", NVS_READWRITE, &nvs_handle);
                nvs_set_u8(nvs_handle, "hw_rev", data[0]);
                nvs_commit(nvs_handle);
                nvs_close(nvs_handle);
                
            }
            uint8_t rev[1] = {0};
            nvs_handle_t nvs_handle;
            nvs_open("storage", NVS_READONLY, &nvs_handle);
            nvs_get_u8(nvs_handle, "hw_rev", rev);
            nvs_commit(nvs_handle);
            nvs_close(nvs_handle);
            m_can.send(ICAN::MSG_ID_t::HW_REV, rev, 1, false);
            return true;
        }
        case ICAN::MSG_ID_t::SENSOR_LEGACY_MODE:
        {
            if (data_len == 1 && !request)
            {
                nvs_handle_t nvs_handle;
                nvs_open("storage", NVS_READWRITE, &nvs_handle);
                nvs_set_u8(nvs_handle, "leg_sen", data[0]);
                nvs_commit(nvs_handle);
                nvs_close(nvs_handle);
                
            }
            uint8_t leg_sen[1] = {0};
            nvs_handle_t nvs_handle;
            nvs_open("storage", NVS_READONLY, &nvs_handle);
            nvs_get_u8(nvs_handle, "leg_sen", leg_sen);
            nvs_commit(nvs_handle);
            nvs_close(nvs_handle);
            m_can.send(ICAN::MSG_ID_t::SENSOR_LEGACY_MODE, leg_sen, 1, false);
            return true;
        }
        default:
            break;
    }
    return false;
}
