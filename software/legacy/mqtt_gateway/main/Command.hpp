#pragma once

#include <cJSON.h>
#include "Logging.hpp"
#include "IMQTT.hpp"
#include "IUpdate.hpp"
#include "WiFi.hpp"
#include "Network.hpp"
#include "Update.hpp"
#include "CANUpdate.hpp"
#include "WebCredentials.hpp"
#include "esp32-ha-lib/ICAN.hpp"

class Command
{
public:
    Command(Update&, CANUpdate&, IMQTT&, ICAN&, WiFi&, Network&, Logging&, WebCredentials&);
    void command(char* cmd, cJSON* root);
    void send_can_command(cJSON* root, ICAN::MSG_ID_t messageId, uint8_t* data,  size_t data_len, 
        bool request);
    void save_config(char* cmd, cJSON* root);
    void relais_rollershutter(char* cmd, cJSON* root);
    void lamps(char* cmd, cJSON* root);
    void mqtt_logging(char* cmd, cJSON* root);
    void web_logging(char* cmd, cJSON* root);
    void save_device(char* cmd, cJSON* root);
    void refresh_device(char* cmd, cJSON* root);
    void legacy_mode(char* cmd, cJSON* root);
    void silence_on(char* cmd, cJSON* root);
    void silence_off(char* cmd, cJSON* root);
    void ping_device(char* cmd, cJSON* root);
    void restart_device(char* cmd, cJSON* root);
    void prepare_update(char* cmd, cJSON* root);
    void complete_update(char* cmd, cJSON* root);
    IUpdate* current_update();

private:
    IUpdate* m_current_update;
    Update& m_selfupdate;
    CANUpdate& m_canupdate;
    IMQTT& m_mqtt;
    ICAN& m_can;
    WiFi& m_wifi;
    Network& m_network;
    Logging& m_logging;
    WebCredentials& m_web_credentials;
    static const char* TAG;
};
