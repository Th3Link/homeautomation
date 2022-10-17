#pragma once

class ICANDispatcher
{
    public:
        virtual void dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request) = 0;
};


class ICAN
{
    public:
        enum class ERROR_t : uint8_t
        {
            FLASH_OVERRUN = 0x1,
            NO_CONFIG = 0x2,
            DEVICE_ID_TYPE_ERROR = 0x3,
            FIRMWARE_CORRUPT = 0x4
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
            FLASH_SELECT = 16,
            FLASH_ERASE = 17,
            FLASH_READ = 18,
            FLASH_WRITE = 19,
            FLASH_VERIFY = 20,
            BUTTON_EVENT = 30,
            TEMPERATURE_SENSOR = 31,
            LAMP_GROUP = 90,
            PIR_SENSOR = 128,
            HUMIDITY_SENSOR = 129,
            RELAIS = 130,
            ROLLERSHUTTER = 131
            //30      button reading
            //31      temperature reading
            //32...56 get lamp state
            //58      gat lamp group state
            //64...88 set lamp command
            //90      set lamp group command
        };
    
        enum class AVAILABLE_t : uint8_t
        {
            NOT_READY = 0,
            APPLICATION = 1,
            UPDATE_MODE = 2
        };
    
        enum class BUTTON_EVENT_t : uint8_t
        {
            RELEASED = 0,
            PRESSED = 1,
            HOLD = 2
        };
    
        enum class DEVICE_t : uint32_t
        {
            Sensor = 1000000,
            Lamp = 2000000,
            Button = 3000000,
            Relais = 4000000,
            TemperatureSensors = 5000000,
            TemperatureSensorsNG = 0x0100,
            RelaisNG = 0x0200,
            Lamps = 0x0300,
            ButtonNG = 0x0400
        };
        
        #pragma pack(push,1)
        struct RELAIS_MSG_t
        {
            uint32_t number : 8;
            uint32_t state : 8;
            uint32_t time : 24;
            uint32_t reserved : 24;
        };
        #pragma pack(pop)
        
        #pragma pack(push,1)
        struct LAMP_MSG_t
        {
            uint32_t value : 8;
            uint32_t bitmask : 24;
            uint32_t reserved : 32;
        };
        #pragma pack(pop)
        
        
        virtual void init() = 0;
        virtual void deinit() = 0;
        virtual void add_dispatcher(ICANDispatcher*) = 0;
        virtual void send(MSG_ID_t, uint8_t* data, 
            unsigned int data_len, bool request) = 0;
        virtual uint8_t get_id();
        virtual uint8_t get_type();
        
};
