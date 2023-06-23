#include "MQTT.hpp"

#include <esp_log.h>
#include <nvs_flash.h>
#include <sstream>
#include <cstring>

const char* MQTT::TAG = "MQTT";

static void mqtt_event_handler(void *handler_args, esp_event_base_t base, 
        int32_t event_id, void *event_data) {
    ESP_LOGD(MQTT::TAG, "Event dispatched from event loop base=%s, event_id=%d", 
        base, static_cast<int>(event_id));

    esp_mqtt_event_handle_t event = reinterpret_cast<esp_mqtt_event_handle_t>(event_data);
    MQTT* mqtt = reinterpret_cast<MQTT*>(handler_args);
    esp_mqtt_client_handle_t client = event->client;
    int msg_id;

    // your_context_t *context = event->context;
    switch (event->event_id) {
        case MQTT_EVENT_CONNECTED:
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_CONNECTED");
            
            mqtt->connected_event();
            
            break;
        case MQTT_EVENT_DISCONNECTED:
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_DISCONNECTED");
            break;

        case MQTT_EVENT_SUBSCRIBED:
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_SUBSCRIBED, msg_id=%d", event->msg_id);
            msg_id = esp_mqtt_client_publish(client, "/topic/qos0", "data", 0, 0, 0);
            ESP_LOGI(MQTT::TAG, "sent publish successful, msg_id=%d", msg_id);
            break;
        case MQTT_EVENT_UNSUBSCRIBED:
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_UNSUBSCRIBED, msg_id=%d", event->msg_id);
            break;
        case MQTT_EVENT_PUBLISHED:
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_PUBLISHED, msg_id=%d", event->msg_id);
            break;
        case MQTT_EVENT_DATA:
        {
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_DATA");
            ESP_LOGI(MQTT::TAG, "TOPIC=%.*s\r\n", event->topic_len, event->topic);
            ESP_LOGI(MQTT::TAG, "DATA=%.*s\r\n", event->data_len, event->data);
            ESP_LOGI(MQTT::TAG, "ID=%d\r\n", event->msg_id);
            
            mqtt->dispatch(event->topic, event->topic_len, event->data, 
                    event->data_len);
            break;
        }
        case MQTT_EVENT_ERROR:
            ESP_LOGI(MQTT::TAG, "MQTT_EVENT_ERROR");
            break;
        default:
            ESP_LOGI(MQTT::TAG, "Other event id:%d", event->event_id);
            break;
    }
}

MQTT::MQTT() : m_uri({0}), m_username({0}), m_password({0}), m_enabled(false),
    m_received(0), m_transmitted(0), m_connected(false)
{
    
}

void MQTT::init()
{
    size_t mqtt_uri_len = sizeof(m_uri);
    size_t mqtt_username_len = sizeof(m_username);
    size_t mqtt_password_len = sizeof(m_password);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    
    uint8_t mqtt_enabled = 0;
    if (nvs_get_u8(nvs_handle, "mqtt_enabled", &mqtt_enabled) != ESP_OK)
    {
        nvs_set_u8(nvs_handle, "mqtt_enabled", mqtt_enabled);
    }
    
    if (nvs_get_str(nvs_handle, "mqtt_uri", &m_uri[0], &mqtt_uri_len) != ESP_OK)
    {
        mqtt_enabled = 0;
        nvs_set_str(nvs_handle, "mqtt_uri", "mqtt://IP:1883");
    }

    if (nvs_get_str(nvs_handle, "mqtt_user", &m_username[0], &mqtt_username_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "mqtt_user", "");
    }
    
    if (nvs_get_str(nvs_handle, "mqtt_pw", &m_password[0], &mqtt_password_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "mqtt_pw", "");
    }
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    
    ESP_LOGI(MQTT::TAG, "MQTT connect to %s:%s@%s", m_username, m_password, m_uri);
    
    esp_mqtt_client_config_t mqtt_cfg;
    memset(&mqtt_cfg, 0, sizeof(mqtt_cfg));
    mqtt_cfg.broker.address.uri = m_uri;
    if (strcmp(m_username, "") != 0)
    {
        mqtt_cfg.credentials.username = m_username;
    }
    
    if (strcmp(m_password, "") != 0)
    {
        mqtt_cfg.credentials.authentication.password = m_password;
    }
    
    ESP_LOGI(MQTT::TAG, "MQTT connect to %s:%s@%s", 
        mqtt_cfg.credentials.username, 
        mqtt_cfg.credentials.authentication.password, 
        mqtt_cfg.broker.address.uri);
    
    
    m_client = esp_mqtt_client_init(&mqtt_cfg);
    esp_mqtt_client_register_event(m_client, static_cast<esp_mqtt_event_id_t>(ESP_EVENT_ANY_ID), 
        mqtt_event_handler, this);
    esp_mqtt_client_start(m_client);
}

void MQTT::subscribe(const char* topic)
{
    esp_mqtt_client_subscribe(m_client, topic, 0);
}

void MQTT::publish(const char* topic, const char* data)
{
    if (connected())
    {
        inc_transmitted();
        esp_mqtt_client_publish(m_client, 
            topic, 
            data, 0, 0, 0);
    }
}

void MQTT::uri(const char* uri, size_t uri_len)
{
    size_t mqtt_uri_len = sizeof(m_uri);
    memset(&m_uri[0], 0, mqtt_uri_len);
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "mqtt_uri", uri);
    nvs_commit(nvs_handle);
    nvs_get_str(nvs_handle, "mqtt_uri", &m_uri[0], &mqtt_uri_len);
    nvs_close(nvs_handle);
}
void MQTT::username(const char* username, size_t username_len)
{
    size_t mqtt_username_len = sizeof(m_username);
    memset(&m_username[0], 0, mqtt_username_len);
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "mqtt_user", username);
    nvs_commit(nvs_handle);
    nvs_get_str(nvs_handle, "mqtt_user", &m_username[0], &mqtt_username_len);
    nvs_close(nvs_handle);
}
void MQTT::password(const char* password, size_t password_len)
{
    size_t mqtt_password_len = sizeof(m_password);
    memset(&m_password[0], 0, mqtt_password_len);
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "mqtt_pw", password);
    nvs_commit(nvs_handle);
    nvs_get_str(nvs_handle, "mqtt_pw", &m_password[0], &mqtt_password_len);
    nvs_close(nvs_handle);
}

void MQTT::enabled(bool e)
{
    uint8_t enabled = static_cast<uint8_t>(e);
    m_enabled = e;
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_u8(nvs_handle, "mqtt_enabled", enabled);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    
}

void MQTT::add_dispatcher(IMQTTDispatcher* dispatcher)
{
    m_dispatcher.push_back(dispatcher);
}

void MQTT::dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len)
{
    inc_received();
    
    try
    {
        for (auto dispatcher : m_dispatcher)
        {
            dispatcher->dispatch(topic, topic_len, data, data_len);
        }
    }
    catch (...)
    {
        std::string topic("canbus/error/gateway_translation");
        std::string data = std::string(topic, topic_len) + " " + std::string(data, data_len);
        publish(topic.c_str(), data.c_str());
    }
}

void MQTT::connected_event()
{
    m_connected = true;
    for (auto dispatcher : m_dispatcher)
    {
        dispatcher->connected_event();
    }
}

const char* MQTT::uri()
{
    return &m_uri[0];
}
const char* MQTT::username()
{
    return &m_username[0];    
}
const char* MQTT::password()
{
    return &m_password[0];
}

bool MQTT::enabled()
{
    return m_enabled;
}

void MQTT::inc_received()
{
    m_received++;
}

void MQTT::inc_transmitted()
{
    m_transmitted++;
}

bool MQTT::connected()
{
    return m_connected;
}

uint64_t MQTT::received()
{
    return m_received;
}

uint64_t MQTT::transmitted()
{
    return m_transmitted;
}
