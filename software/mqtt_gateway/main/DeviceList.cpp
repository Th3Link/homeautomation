#include "DeviceList.hpp"
#include "helper.hpp"
#include <chrono>
#include <cstdio>
#include <cstring>

#include <cJSON.h>
#include <esp_timer.h>
#include <esp_log.h>

const char* DeviceList::TAG = "DeviceList";
static esp_timer_handle_t ping_timer;
static esp_timer_handle_t broadcast_timer;
DeviceList::DeviceList(ICAN& ic) : m_can(ic)
{
    for (unsigned int i = 0; i < DEVICE_LIST_SIZE; i++)
    {
        m_deviceList[i].id = 0;
    }
    m_can.add_dispatcher(this);
}

static void ping_timer_callback(void* arg)
{
    #pragma pack(push,1)
    static union {
        uint8_t data[4];
        uint32_t data32;
    } data_union {0};
    #pragma pack(pop)
    ICAN* can = reinterpret_cast<ICAN*>(arg);
    can->send(ICAN::MSG_ID_t::PING, data_union.data, sizeof(data_union.data), false);
    data_union.data32++;
}

static void broadcast_timer_callback(void* arg)
{
    ICAN* can = reinterpret_cast<ICAN*>(arg);
    can->send(0x10000000 + static_cast<uint32_t>(ICAN::MSG_ID_t::AVAILABLE), NULL, 0, true);
}

void DeviceList::init()
{
    const esp_timer_create_args_t ping_timer_args = {
            .callback = &ping_timer_callback,
            .arg = &m_can,
            .dispatch_method = ESP_TIMER_TASK,
            .name = "ping",
            .skip_unhandled_events = true
    };
    ESP_ERROR_CHECK(esp_timer_create(&ping_timer_args, &ping_timer));
    
    const esp_timer_create_args_t broadcast_timer_args = {
            .callback = &broadcast_timer_callback,
            .arg = &m_can,
            .dispatch_method = ESP_TIMER_TASK,
            .name = "broadcast",
            .skip_unhandled_events = true
    };
    ESP_ERROR_CHECK(esp_timer_create(&broadcast_timer_args, &broadcast_timer));

    ESP_ERROR_CHECK(esp_timer_start_periodic(ping_timer, 2000000));
    ESP_ERROR_CHECK(esp_timer_start_periodic(broadcast_timer, 10000000));
}

static void update_device(DeviceList::DeviceListEntry& device, uint32_t identifier, 
    uint8_t* data, unsigned int data_len)
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
            for (unsigned int i = 0; i < std::min(data_len,static_cast<unsigned int>(12-1)); i++)
            {
                device.version[i] = data[i];
            }
            device.version[data_len] = 0;
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
        case ICAN::MSG_ID_t::HW_REV:
            device.hwrev = data[0];
            break;
        case ICAN::MSG_ID_t::SENSOR_LEGACY_MODE:
            device.legacy_sensor = data[0];
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

std::string DeviceList::entry(uint32_t id)
{
    for (unsigned int i = 0; i < DEVICE_LIST_SIZE; i++)
    {
        if (m_deviceList[i].id == id)
        {
            return std::string(m_deviceList[i].custom_string.data());
        }
    }
    return "";
}

uint32_t DeviceList::resolve(std::string device_string)
{
    std::string prefix = "0x";
    if (std::equal(prefix.begin(), prefix.end(), device_string.begin()))
    {
        return hextoInt(device_string);
    }
    
    for (unsigned int i = 0; i < DEVICE_LIST_SIZE; i++)
    {
        if (m_deviceList[i].id != 0)
        {
            std::string cs(m_deviceList[i].custom_string.data());
            if (cs == device_string)
            {
                return m_deviceList[i].id;
            }
        }
    }
    return 0;
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
            char uid0[22] {0};
            char uid1[22] {0};
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
            cJSON_AddNumberToObject(device, "hwrev", m_deviceList[i].hwrev);
            cJSON_AddNumberToObject(device, "legacy_sensor", m_deviceList[i].hwrev);
            cJSON_AddStringToObject(device, "state", &state[0]);
            cJSON_AddStringToObject(device, "last_error", &error[0]);
            cJSON_AddItemToArray(devices, device);
        }
    }
}
