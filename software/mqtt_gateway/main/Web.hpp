#pragma once

#include "IUpdate.hpp"
#include "Update.hpp"
#include "CANUpdate.hpp"
#include "Command.hpp"
#include "IMQTT.hpp"
#include "WiFi.hpp"
#include "Logging.hpp"
#include "DeviceList.hpp"
#include "esp32-ha-lib/ICAN.hpp"
#include "esp_http_server.h"

class Web
{
public:
    Web(Update&, CANUpdate&, IMQTT&, ICAN&, WiFi&, Logging&, DeviceList&);
    void init();

    esp_err_t state_get_handler(httpd_req_t *req);
    esp_err_t control_post_handler(httpd_req_t *req);
    esp_err_t update_data_post_handler(httpd_req_t *req);
private:
    static const char* TAG;
    static constexpr size_t SCRATCH_BUFSIZE = 10240;
    char scratch[SCRATCH_BUFSIZE];
    char username[30];
    char password[30];
    Command m_command;
    IMQTT& m_mqtt;
    ICAN& m_can;
    WiFi& m_wifi;
    Logging& m_logging;
    DeviceList& m_deviceList;
};
