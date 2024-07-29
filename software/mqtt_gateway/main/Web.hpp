#pragma once

#include "IUpdate.hpp"
#include "Update.hpp"
#include "CANUpdate.hpp"
#include "Command.hpp"
#include "IMQTT.hpp"
#include "WiFi.hpp"
#include "Network.hpp"
#include "Logging.hpp"
#include "DeviceList.hpp"
#include "WebCredentials.hpp"
#include "esp32-ha-lib/ICAN.hpp"
#include "esp_http_server.h"

class Web
{
public:
    Web(Update&, CANUpdate&, IMQTT&, ICAN&, WiFi&, Network&, Logging&, DeviceList&);
    void init();
    const char* username();
    const char* password();
    esp_err_t state_get_handler(httpd_req_t *req);
    esp_err_t control_post_handler(httpd_req_t *req);
    esp_err_t update_data_post_handler(httpd_req_t *req);
    esp_err_t bridge_config_json_get_handler(httpd_req_t *req);
    esp_err_t bridge_config_json_put_handler(httpd_req_t *req);
private:
    static const char* TAG;
    static constexpr size_t SCRATCH_BUFSIZE = 10240;
    char scratch[SCRATCH_BUFSIZE];
    Command m_command;
    CANUpdate& m_canupdate;
    IMQTT& m_mqtt;
    ICAN& m_can;
    WiFi& m_wifi;
    Network& m_network;
    Logging& m_logging;
    DeviceList& m_deviceList;
    WebCredentials m_web_credentials;
};
