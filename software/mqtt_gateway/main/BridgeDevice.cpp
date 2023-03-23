#include "BridgeDevice.hpp"
#include "helper.hpp"

const char* BridgeDevice::TAG = "BridgeDevice";
const char canbusavailable_topic[] = "canbus/available/";
BridgeDevice::BridgeDevice(ICAN& ic, IMQTT& im, DeviceList& dl) : m_can(ic), m_mqtt(im), m_device_list(dl)
{
    //m_mqtt.add_dispatcher(this);
    m_can.add_dispatcher(this);
}

void BridgeDevice::init()
{

}

std::string BridgeDevice::sensor_name(ICAN::MSG_ID_t msg_id)
{
    switch (msg_id)
    {
        case ICAN::MSG_ID_t::TEMPERATURE_SENSOR:
            return "temperature";
        case ICAN::MSG_ID_t::PRESSURE_SENSOR:
            return "pressure";
        case ICAN::MSG_ID_t::HUMIDITY_SENSOR:
            return "humidity";
        case ICAN::MSG_ID_t::CO2_EQUIVALENT:
            return "co2";
        case ICAN::MSG_ID_t::VOC_BREATH:
            return "voc";
        case ICAN::MSG_ID_t::AIR_QUALITY:
            return "air_quality";
        case ICAN::MSG_ID_t::AMBIENT_LIGHT_SENSOR:
            return "brightness";
        default:
            return "unknown";
    }
}

BridgeDevice::sensor_data_t BridgeDevice::sensor_value(uint32_t identifier, 
    uint8_t* data, unsigned int data_len)
{
    ICAN::MSG_ID_t msg_id = ICAN::GET_MSG(identifier);
    sensor_data_t sensor_data;
    sensor_data.name = sensor_name(msg_id);
    sensor_data.id = "/0x" + toHexString(ICAN::GET_NOT_MSG(identifier));
    sensor_data.value = 0.0;
    sensor_data.custom_string = "/" + m_device_list.entry(identifier & 0xFFFFFF00);
    switch (msg_id)
    {
        case ICAN::MSG_ID_t::TEMPERATURE_SENSOR:
            //fall through
        case ICAN::MSG_ID_t::PRESSURE_SENSOR:
            //fall through
        case ICAN::MSG_ID_t::HUMIDITY_SENSOR:
            //fall through
        case ICAN::MSG_ID_t::CO2_EQUIVALENT:
            //fall through
        case ICAN::MSG_ID_t::VOC_BREATH:
            //fall through
        case ICAN::MSG_ID_t::AIR_QUALITY:
        {
            struct sensor_value_t
            {
                uint64_t id : 48;
                uint64_t value : 16;
            };
            
            union {
                uint8_t data[8];
                sensor_value_t sv;
            } t;

            for (auto i = 0; i < std::min(data_len,static_cast<unsigned int>(8)); i++)
            {
                t.data[i] = data[i];
            };
            
            sensor_data.value = t.sv.value/16.0;
            sensor_data.additional_id = "/" + toHexString(t.sv.id);
            break;
        }
        case ICAN::MSG_ID_t::AMBIENT_LIGHT_SENSOR:
        {
            union {
                uint8_t data[4];
                uint32_t data32;
            } t;

            for (auto i = 0; i < std::min(data_len,static_cast<unsigned int>(4)); i++)
            {
                t.data[i] = data[i];
            };
            sensor_data.value = t.data32/1.0;
            break;
        }
        default:
            break;
    }
    return sensor_data;
}

bool BridgeDevice::dispatch_sensor(sensor_data_t sensor_data)
{

    //temperature sensors
    std::string sensorTopic1 = "canbus/" + sensor_data.name;
    std::string sensorTopic_id = sensorTopic1 + sensor_data.id + sensor_data.additional_id;
    std::string sensorData = std::to_string(sensor_data.value);

    if (sensor_data.custom_string.length() > 1)
    {
        std::string sensorTopic_cs = sensorTopic1 + sensor_data.custom_string 
            + sensor_data.additional_id;
        m_mqtt.publish(sensorTopic_cs.c_str(), sensorData.c_str());
    }

    m_mqtt.publish(sensorTopic_id.c_str(), sensorData.c_str());
    return true;
}

bool BridgeDevice::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::TEMPERATURE_SENSOR)
        || ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::HUMIDITY_SENSOR)
        || ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::PRESSURE_SENSOR)
        || ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::AIR_QUALITY)
        || ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::CO2_EQUIVALENT)
        || ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::VOC_BREATH)
        || ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::AMBIENT_LIGHT_SENSOR))
    {
        return dispatch_sensor(sensor_value(identifier, data, data_len));
    }

    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::AVAILABLE))
    {
        std::string availableTopic = std::string(canbusavailable_topic)
        + toHexString(identifier);
    }
    return false;
}

void BridgeDevice::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{

}

void BridgeDevice::connected_event()
{
    std::string availableTopic = std::string(canbusavailable_topic)
    + toHexString(ICAN::DID_TO_ID(m_can.get_id()) + ICAN::TYPE_TO_ID(static_cast<ICAN::DEVICE_t>(m_can.get_type())) + 
        ICAN::ID_NG_MASK);
    std::string availableData = "0x01";
    m_mqtt.publish(availableTopic.c_str(), availableData.c_str());
}
