#include "DeviceList.hpp"
#include <chrono>
#include <cstdio>
#include <cstring>

#include <cJSON.h>
#include <esp_timer.h>
#include <esp_log.h>

const char* DeviceList::TAG = "DeviceList";

DeviceList::DeviceList(ICAN& ic) : m_can(ic)
{
    for (unsigned int i = 0; i < 50; i++)
    {
        m_deviceList[i].id = 0;
    }
    m_can.add_dispatcher(this);
}

void DeviceList::init()
{

}

static void update_device(DeviceList::DeviceListEntry& device, uint32_t identifier, uint8_t* data, unsigned int data_len)
{
    uint32_t mins = std::chrono::duration_cast<std::chrono::minutes>(
        std::chrono::microseconds(esp_timer_get_time())).count();
    
    device.last_seen = mins;
    switch (static_cast<ICAN::MSG_ID_t>(identifier))
    {
        case ICAN::MSG_ID_t::AVAILABLE:
            device.state = data[0];
            break;
        case ICAN::MSG_ID_t::DEVICE_ERROR:
            device.error = data[0];
            break;
        case ICAN::MSG_ID_t::BAUDRATE:
            device.baudrate = data[0];
            break;
        case ICAN::MSG_ID_t::DEVICE_UID0:
        {
            for (unsigned int i = 0; i < data_len; i++)
            {
                device.uid0[i] = data[i];
            }
            break;
        }
        case ICAN::MSG_ID_t::DEVICE_UID1:
        {
            for (unsigned int i = 0; i < data_len; i++)
            {
                device.uid1[i] = data[i];
            }
            break;
        }
        // application version in binary form
        case ICAN::MSG_ID_t::APPLICATION_VERSION:
        {
            if (data_len == 4)
            {
                union {
                    struct {
                        uint16_t major;
                        uint16_t minor;
                    } s;
                    uint8_t data8[4];
                } u;
                for (unsigned int i = 0; i < data_len; i++)
                {
                    u.data8[i] = data[i];
                }
                sprintf(&device.version[0], "%d.%d", u.s.major, u.s.minor);
            }
            break;
        }
        case ICAN::MSG_ID_t::APPLICATION_VERSION_STRING:
        {
            for (unsigned int i = 0; i < data_len; i++)
            {
                device.version[i] = data[i];
            }
            break;
        }
        case ICAN::MSG_ID_t::UPTIME:
        {
            union
            {
                uint32_t t;
                uint8_t t8[4];
            };
            
            for (unsigned int i = 0; i < data_len; i++)
            {
                t8[i] = data[i];
            }
            device.uptime = t;
            break;
        }
        case ICAN::MSG_ID_t::CUSTOM_STRING:
        {
            for (unsigned int i = 0; i < data_len; i++)
            {
                device.custom_string[i] = data[i];
            }
            break;
        }
        case ICAN::MSG_ID_t::ROLLERSHUTTER_MODE:
            device.rollershutter_mode = data[0];
            break;
        default:
            break;
    }
}

bool DeviceList::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    // return always false --> do not consume the msg
    if (ICAN::GET_ID(identifier) == 0)
    {
        return false;
    }

    for (unsigned int i = 0; i < 50; i++)
    {
        if (m_deviceList[i].id == (identifier & 0xFFFFFF00))
        {
            update_device(m_deviceList[i], identifier, data, data_len);
            return false;
        }
    }
    
    // create new entry
    for (unsigned int i = 0; i < 50; i++)
    {
        if (m_deviceList[i].id == 0)
        {
            m_deviceList[i].id = (identifier & 0xFFFFFF00);
            uint8_t data[1];
            m_can.send((identifier & 0xFFFFFF00)+12, &data[0], 0, true);
            return false;
        }
    }
    return false;
}

void DeviceList::refresh()
{
    uint8_t data[1];
    m_can.send(0x10000000, &data[0], 0, true);
    
}

void DeviceList::output(cJSON* object)
{
    cJSON *devices = cJSON_AddArrayToObject(object, "devices");
    for (unsigned int i = 0; i < 50; i++)
    {
        if (m_deviceList[i].id != 0)
        {
            char uid[30] {0};
            char device_id[5] {0};
            char device_type[5] {0};
            char custom_string[9] {0};
            char state[5] {0};
            char error[5] {0};
            char uid0[20] {0};
            char uid1[20] {0};
            char version[9] {0};
            char rollershutter_mode[9] {0};
            
            sprintf(&uid[0], "0x%08lx", m_deviceList[i].id);
            uint8_t deviceid = static_cast<uint8_t>((m_deviceList[i].id & 0xFF00) >> 8);
            uint8_t devicetype = static_cast<uint8_t>((m_deviceList[i].id & 0x3F0000) >> 16);
            sprintf(&device_id[0], "0x%02x", deviceid);
            sprintf(&device_type[0], "0x%02x", devicetype);
            
            sprintf(uid0, "0x");
            sprintf(uid1, "0x");
            
            for (unsigned int j = 0; j < 8; j++)
            {
                char part[3] {0};
                sprintf(part, "%02x", m_deviceList[i].uid0[j]);
                strcat(uid0, part);
                sprintf(part, "%02x", m_deviceList[i].uid1[j]);
                strcat(uid1, part);
            }
            
            for (unsigned int j = 0; j < 8; j++)
            {
                custom_string[j] = m_deviceList[i].custom_string[j];
            }
            
            for (unsigned int j = 0; j < 8; j++)
            {
                version[j] = m_deviceList[i].version[j];
            }
                       
            if (m_deviceList[i].rollershutter_mode == 2)
            {
                sprintf(&rollershutter_mode[0], "HARDWARE");
            }
            else
            {
                sprintf(&rollershutter_mode[0], "SOFTWARE");
            }
            
            sprintf(&state[0], "0x%02x", m_deviceList[i].state);
            sprintf(&error[0], "0x%02x", m_deviceList[i].error);
            
            uint32_t mins = std::chrono::duration_cast<std::chrono::minutes>(
                std::chrono::microseconds(esp_timer_get_time())).count();
            
            uint32_t ls = mins - m_deviceList[i].last_seen;
            
            cJSON* device = cJSON_CreateObject();
            cJSON_AddStringToObject(device, "uid", &uid[0]);
            cJSON_AddStringToObject(device, "device_id", &device_id[0]);
            cJSON_AddStringToObject(device, "device_type", &device_type[0]);
            cJSON_AddStringToObject(device, "device_type_name", ICAN::device_string(
                static_cast<ICAN::DEVICE_t>(devicetype)));
            cJSON_AddStringToObject(device, "custom_string", &custom_string[0]);
            cJSON_AddStringToObject(device, "uid0", &uid0[0]);
            cJSON_AddStringToObject(device, "uid1", &uid1[0]);
            cJSON_AddStringToObject(device, "version", &version[0]);
            cJSON_AddNumberToObject(device, "uptime", m_deviceList[i].uptime);
            cJSON_AddStringToObject(device, "baudrate", ICAN::bitrate_string(
                static_cast<ICAN::BITRATE_t>(m_deviceList[i].baudrate)));
            cJSON_AddStringToObject(device, "rollershutter_mode", &rollershutter_mode[0]);
            cJSON_AddNumberToObject(device, "last_seen", ls);
            cJSON_AddStringToObject(device, "state", &state[0]);
            cJSON_AddStringToObject(device, "last_error", &error[0]);
            cJSON_AddItemToArray(devices, device);
        }
    }
}
