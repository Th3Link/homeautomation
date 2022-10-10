#include <nvs_flash.h>
#include <esp_mac.h>
#include "Device.hpp"
#include <chrono>

const char* Device::TAG = "Device";

Device::Device(ICAN& ic): m_can(ic)
{
    m_can.add_dispatcher(this);
}

void Device::init()
{
    
}

void Device::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    static bool uid_selected = false;
    switch (static_cast<ICAN::MSG_ID_t>(identifier & 0xFF))
    {
        case ICAN::MSG_ID_t::DEVICE_GROUP:
        {
            if (request)
            {
                uint8_t data[1] {0};
                m_can.send(ICAN::MSG_ID_t::DEVICE_GROUP, data, sizeof(data), false);
            }
            break;
        }

        case ICAN::MSG_ID_t::APPLICATION_VERSION:
        {
            if (request)
            {
                uint8_t data[6] {0};
                m_can.send(ICAN::MSG_ID_t::APPLICATION_VERSION, data, sizeof(data), false);
            }
            break;
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

                nvs_set_u8(nvs_handle, "m_can_type", data[1]);
                nvs_set_u8(nvs_handle, "m_can_id", data[0]);

                nvs_close(nvs_handle);
                m_can.deinit();
                m_can.init();
            }
            break;
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
                    for (unsigned int i = 0; i < 8; i++)
                    {
                        if (chipid[i] != data[i])
                        {
                            uid_selected = false;
                        }
                    }
                }
            }
            break;
        }
        case ICAN::MSG_ID_t::CUSTOM_STRING:
        {
            if (request)
            {
                // custom string
                // uptime
                
                uint8_t custom_string[8] {0};

                m_can.send(ICAN::MSG_ID_t::CUSTOM_STRING, custom_string, 
                    sizeof(custom_string), false);
            }
            break;
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
                
                auto uptime_auto = std::chrono::duration_cast<std::chrono::seconds>(
                      std::chrono::system_clock::now().time_since_epoch()).count();
                
                uptime = static_cast<decltype(uptime)>(uptime_auto);

                m_can.send(ICAN::MSG_ID_t::UPTIME, uptime8, 
                    sizeof(uptime8), false);
            }
        }
        default:
            break;
    }
}
