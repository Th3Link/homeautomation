#pragma once

#include <cstdint>
#include <vector>
#include "mqtt_client.h"
#include "IMQTT.hpp"

class MQTT : public IMQTT
{
public:
    MQTT();
    void init();
    void subscribe(const char* topic) override;
    void publish(const char* topic, const char* data) override;
    std::vector<IMQTTDispatcher*> dispatcher();
    void add_dispatcher(IMQTTDispatcher*) override;
    void uri(const char*, size_t) override;
    void username(const char*, size_t) override;
    void password(const char*, size_t) override;
    const char* uri() override;
    const char* username() override;
    const char* password() override;
    bool enabled() override;
    void enabled(bool) override;
    void inc_received();
    void inc_transmitted();
    void connected(bool);
    bool connected() override;
    uint64_t received() override;
    uint64_t transmitted() override;
    static const char* TAG;
private:
    char m_uri[60];
    char m_username[60];
    char m_password[60];
    bool m_enabled;
    uint64_t m_received;
    uint64_t m_transmitted;
    bool m_connected;
    esp_mqtt_client_handle_t m_client;
    std::vector<IMQTTDispatcher*> m_dispatcher;
};
