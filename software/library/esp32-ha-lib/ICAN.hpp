#pragma once

#include <cstring>
#include <string>
#include <cstdint>

class ICANDispatcher
{
    public:
        virtual bool dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) = 0;
};


class ICAN
{
public:
    enum class BITRATE_t : uint8_t
    {
        BITRATE_22_222 = 1,
        BITRATE_25 = 2,
        BITRATE_50 = 3,
        BITRATE_100 = 4
    };

    enum class ERROR_t : uint8_t
    {
        FLASH_OVERRUN = 0x01,
        NO_CONFIG = 0x02,
        DEVICE_ID_TYPE_ERROR = 0x03,
        FIRMWARE_CORRUPT = 0x04,
        COMPONENT_CAN = 0x05,
        COMPONENT_LIGHT = 0x06,
        COMPONENT_RELAIS = 0x07,
        COMPONENT_MAIN = 0x08,
        COMPONENT_UPDATE = 0x09,
        COMPONENT_NIGHTLIGHT = 0x0A,
        COMPONENT_AMBIENT = 0x0B,
    };

    enum class MSG_ID_t : uint8_t
    {
        AVAILABLE = 0,
        DEVICE_ERROR = 1,
        RESTART = 2,
        DEVICE_UID0 = 3,
        DEVICE_UID1 = 4,
        DEVICE_ID_TYPE = 5,
        DEVICE_GROUP = 6,
        APPLICATION_VERSION = 7,
        BAUDRATE = 8,
        UPTIME = 9,
        CUSTOM_STRING = 10,
        PWM_FREQUENCY = 11,
        REQUEST_PARAMETER = 12,
        APPLICATION_VERSION_STRING = 13,
        UPDATE_SILENCE = 14,
        FLASH_SELECT = 16,
        FLASH_ERASE = 17,
        FLASH_READ = 18,
        FLASH_WRITE = 19,
        FLASH_VERIFY = 20,
        BUTTON_EVENT = 30,
        TEMPERATURE_SENSOR = 31,
        HW_REV = 41,
        SENSOR_LEGACY_MODE = 42,
        LAMP_GROUP = 90,
        PIR_SENSOR = 128,
        HUMIDITY_SENSOR = 129,
        RELAIS = 130,
        RELAIS_STATE = 131,
        ROLLERSHUTTER = 132,
        ROLLERSHUTTER_STATE = 133,
        ROLLERSHUTTER_MODE = 134,
        AMBIENT_LIGHT_SENSOR = 140,
        AMBIENT_LIGHT_SENSOR_WHITE = 141,
        NIGHTLIGHT = 150,
        PRESSURE_SENSOR = 151,
        CO2_EQUIVALENT = 152,
        VOC_BREATH = 153,
        AIR_QUALITY = 154,
        LOG_DOWNLOAD = 155,
        PING = 156,
        PING_DISABLE = 157,
    };

    enum class AVAILABLE_t : uint8_t
    {
        NOT_READY = 0,
        APPLICATION = 1,
        UPDATE_MODE = 2
    };

    enum class SILENCE_t : uint8_t
    {
        SILENCE_OFF = 0,
        SILENCE_ON = 1
    };

    enum class BUTTON_EVENT_t : uint8_t
    {
        RELEASED = 0,
        PRESSED = 1,
        HOLD = 2,
        SINGLE = 3,
        DOUBLE = 4,
        TRIPPLE = 5
    };

    enum class DEVICE_t : uint32_t
    {
        Unknown = 0x00,
        LegacyRelais = 0x02,
        LegacyLamps = 0x03,
        Button = 0x04,
        Relais = 0x05,
        Gateway = 0x06,
        Rollershutter = 0x07,
        SSR = 0x08
    };
    
    enum class ROLLERSHUTTER_MODE_t : uint8_t
    {
        SOFTWARE = 1,
        HARDWARE = 2
    };
    
    #pragma pack(push,1)
    struct RELAIS_MSG_t
    {
        uint32_t number : 8;
        uint32_t state : 8;
        uint32_t time : 24;
        uint32_t bank : 8;
        uint32_t reserved : 16;
    };
    #pragma pack(pop)
    
    #pragma pack(push,1)
    struct LAMP_MSG_t
    {
        uint32_t value : 8;
        uint32_t bitmask : 24;
        uint32_t bank : 8;
        uint32_t reserved : 24;
    };
    #pragma pack(pop)

    virtual void add_dispatcher(ICANDispatcher*) = 0;
    virtual void send(MSG_ID_t, uint8_t* data, 
        unsigned int data_len, bool request) = 0;
    virtual void send(uint32_t, uint8_t* data, 
        unsigned int data_len, bool request) = 0;
    virtual uint8_t get_id() = 0;
    virtual uint8_t get_type() = 0;
    virtual void bitrate(ICAN::BITRATE_t) = 0;
    virtual ICAN::BITRATE_t bitrate() = 0;
    virtual uint64_t received() = 0;
    virtual uint64_t transmitted() = 0;

    static inline ICAN::DEVICE_t device_type(std::string s)
    {
        if (s == "LegacyRelais")
        {
            return ICAN::DEVICE_t::LegacyRelais;
        }
        else if (s == "LegacyLamps")
        {
            return ICAN::DEVICE_t::LegacyLamps;
        }
        else if (s == "Button")
        {
            return ICAN::DEVICE_t::Button;
        }
        else if (s == "Relais")
        {
            return ICAN::DEVICE_t::Relais;
        }
        else if (s == "Gateway")
        {
            return ICAN::DEVICE_t::Gateway;
        }
        else if (s == "Rollershutter")
        {
            return ICAN::DEVICE_t::Rollershutter;
        }
        else if (s == "SSR")
        {
            return ICAN::DEVICE_t::SSR;
        }
        return ICAN::DEVICE_t::Unknown;
    }

    static inline const char* device_string(ICAN::DEVICE_t b)
    {
        switch (b)
        {
            case ICAN::DEVICE_t::LegacyRelais:
                return "LegacyRelais";
            case ICAN::DEVICE_t::LegacyLamps:
                return "LegacyLamps";
            case ICAN::DEVICE_t::Button:
                return "Button";
            case ICAN::DEVICE_t::Relais:
                return "Relais";
            case ICAN::DEVICE_t::Gateway:
                return "Gateway";
            case ICAN::DEVICE_t::Rollershutter:
                return "Rollershutter";
            case ICAN::DEVICE_t::SSR:
                return "SSR";
            case ICAN::DEVICE_t::Unknown:
                return "Unknown";
        }
        return "Unknown";
    }

    static inline const char* bitrate_string(ICAN::BITRATE_t b)
    {
        switch (b)
        {
            case ICAN::BITRATE_t::BITRATE_22_222:
                return "b22_222";
            case ICAN::BITRATE_t::BITRATE_25:
                return "b25";
            case ICAN::BITRATE_t::BITRATE_50:
                return "b50";
            case ICAN::BITRATE_t::BITRATE_100:
                return "b100";
        }
        return "b50";
    }

    static inline ICAN::BITRATE_t bitrate(const char* c)
    {
        if (strcmp(c, "b22_222") == 0)
        {
            return ICAN::BITRATE_t::BITRATE_22_222;
        }
        else if (strcmp(c, "b25") == 0)
        {
            return ICAN::BITRATE_t::BITRATE_25;
        }
        else if (strcmp(c, "b50") == 0)
        {
            return ICAN::BITRATE_t::BITRATE_50;
        }
        else if (strcmp(c, "b100") == 0)
        {
            return ICAN::BITRATE_t::BITRATE_100;
        }
        return ICAN::BITRATE_t::BITRATE_50;
    }
    
    static inline ICAN::BITRATE_t bitrate(unsigned int i)
    {
        if (i == 22222)
        {
            return ICAN::BITRATE_t::BITRATE_22_222;
        }
        else if (i == 25000)
        {
            return ICAN::BITRATE_t::BITRATE_25;
        }
        else if (i == 50000)
        {
            return ICAN::BITRATE_t::BITRATE_50;
        }
        else if (i == 100000)
        {
            return ICAN::BITRATE_t::BITRATE_100;
        }
        return ICAN::BITRATE_t::BITRATE_50;
    }
    
    static constexpr uint32_t ID_NG_MASK       = 0x10000000; // 1 bit
    static constexpr uint32_t ID_GROUP_MASK    = 0x0FC00000; // 6 bits
    static constexpr uint32_t ID_TYPE_MASK     = 0x003F0000; // 6 bits
    static constexpr uint32_t ID_ID_MASK       = 0x0000FF00; // 8 bits
    static constexpr uint32_t ID_MSG_MASK      = 0x000000FF; // 8 bits

    static constexpr uint32_t GET_GROUP(uint32_t id)
    {
        return (id & ID_GROUP_MASK) >> 22;
    }

    static constexpr uint32_t GET_TYPE(uint32_t id)
    {
        return (id & ID_TYPE_MASK) >> 16;
    }

    static constexpr uint32_t TYPE_TO_ID(DEVICE_t type)
    {
        return ((static_cast<uint8_t>(type) << 16) & ID_TYPE_MASK);
    }

    static constexpr uint32_t DID_TO_ID(uint8_t did)
    {
        return ((static_cast<uint8_t>(did) << 8) & ID_ID_MASK);
    }

    static constexpr uint32_t GET_ID(uint32_t id)
    {
        return (id & ID_ID_MASK) >> 8;
    }

    static constexpr MSG_ID_t GET_MSG(uint32_t id)
    {
        return static_cast<MSG_ID_t>(id & ID_MSG_MASK);
    }

    static constexpr uint32_t GET_NOT_MSG(uint32_t id)
    {
        return id & ~ID_MSG_MASK;
    }

    static constexpr bool TYPE_COMPARE(uint32_t id, uint8_t type)
    {
        return ((id & ID_NG_MASK) && (GET_TYPE(id) == type));
    }

    static constexpr bool ID_COMPARE(uint32_t id, uint8_t device_id)
    {
        return ((id & ID_NG_MASK) && (GET_ID(id) == device_id));
    }

    static constexpr bool MSG_COMPARE(uint32_t id, MSG_ID_t msg_id)
    {
        return ((id & ID_NG_MASK) && (static_cast<MSG_ID_t>(GET_MSG(id)) == msg_id));
    }
};
