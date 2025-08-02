#include "Command.hpp"

#include <algorithm>

#include <esp_log.h>
#include <esp_system.h>
#include <nvs_flash.h>

const char *Command::TAG = "Command";

Command::Command(Update &u, CANUpdate &cu, IMQTT &im, ICAN &ic, WiFi &w, Network &n, Logging &l,
                 WebCredentials &web)
    : m_current_update(nullptr), m_selfupdate(u), m_canupdate(cu), m_mqtt(im), m_can(ic), m_wifi(w),
      m_network(n), m_logging(l), m_web_credentials(web) {}

void Command::command(char *cmd, cJSON *root) {
    save_config(cmd, root);
    relais_rollershutter(cmd, root);
    lamps(cmd, root);
    mqtt_logging(cmd, root);
    save_device(cmd, root);
    refresh_device(cmd, root);
    ping_device(cmd, root);
    restart_device(cmd, root);
    prepare_update(cmd, root);
    complete_update(cmd, root);
    legacy_mode(cmd, root);
    silence_on(cmd, root);
    silence_off(cmd, root);
}

void Command::send_can_command(cJSON *root, ICAN::MSG_ID_t messageId, uint8_t *data,
                               size_t data_len, bool request) {
    char *unit = cJSON_GetStringValue(cJSON_GetObjectItem(root, "unit"));
    if (strcmp(unit, "can_all") == 0) {
        m_can.send(0x10000000 + static_cast<uint32_t>(messageId), data, data_len, request);
    } else if (strcmp(unit, "can_by_type") == 0) {
        char *cid = cJSON_GetStringValue(cJSON_GetObjectItem(root, "commandId"));
        uint8_t type = std::stoul(std::string(cid), nullptr, 16) & 0xFF;
        m_can.send(0x10000000 + static_cast<uint32_t>(messageId) + (type << 16), data, data_len,
                   request);
    } else if (strcmp(unit, "can_selected") == 0) {
        cJSON *commandId = cJSON_GetObjectItem(root, "commandId");
        if (cJSON_IsArray(commandId)) {
            int size = cJSON_GetArraySize(commandId);
            for (unsigned int i = 0; i < size; i++) {
                char *cid = cJSON_GetStringValue(cJSON_GetArrayItem(commandId, i));
                uint32_t uid = std::stoul(std::string(cid), nullptr, 16);
                m_can.send(uid + static_cast<uint32_t>(messageId), data, data_len, request);
            }
        }
    } else if (strcmp(unit, "can_by_uid") == 0) {
        char *cid = cJSON_GetStringValue(cJSON_GetObjectItem(root, "commandId"));
        uint32_t uid = std::stoul(std::string(cid), nullptr, 16);
        m_can.send(uid + static_cast<uint32_t>(messageId), data, data_len, request);
    }
}

void Command::save_config(char *cmd, cJSON *root) {
    if (strcmp(cmd, "save_config") != 0) {
        return;
    }

    nvs_handle_t nvs_handle;
    ESP_ERROR_CHECK(nvs_open("storage", NVS_READWRITE, &nvs_handle));

    cJSON *hostname = cJSON_GetObjectItem(root, "hostname");
    if (cJSON_IsString(hostname)) {
        m_wifi.hostname(cJSON_GetStringValue(hostname));
    }

    cJSON *username = cJSON_GetObjectItem(root, "username");
    if (cJSON_IsString(username)) {
        m_web_credentials.username(cJSON_GetStringValue(username));
    }

    cJSON *password = cJSON_GetObjectItem(root, "password");
    if (cJSON_IsString(password)) {
        m_web_credentials.password(cJSON_GetStringValue(password));
    }
    cJSON *update_delay_str = cJSON_GetObjectItem(root, "update_delay");
    if (cJSON_IsString(update_delay_str)) {
        m_canupdate.update_delay(cJSON_GetStringValue(update_delay_str));
    }

    cJSON *wifi = cJSON_GetObjectItem(root, "wifi");
    if (cJSON_IsObject(wifi)) {
        cJSON *mode = cJSON_GetObjectItem(wifi, "mode");
        cJSON *ssid = cJSON_GetObjectItem(wifi, "ssid");
        cJSON *password = cJSON_GetObjectItem(wifi, "password");

        if (cJSON_IsString(mode)) {
            m_wifi.mode_str(cJSON_GetStringValue(mode));
        }

        if (cJSON_IsString(ssid)) {
            m_wifi.ssid(cJSON_GetStringValue(ssid));
        }

        if (cJSON_IsString(password)) {
            m_wifi.password(cJSON_GetStringValue(password));
        }
    }

    cJSON *mqtt = cJSON_GetObjectItem(root, "mqtt");
    if (cJSON_IsObject(mqtt)) {
        cJSON *uri = cJSON_GetObjectItem(mqtt, "uri");
        cJSON *username = cJSON_GetObjectItem(mqtt, "username");
        cJSON *password = cJSON_GetObjectItem(mqtt, "password");
        cJSON *enabled = cJSON_GetObjectItem(mqtt, "enabled");

        if (cJSON_IsString(uri)) {
            char *uri_string = cJSON_GetStringValue(uri);
            m_mqtt.uri(uri_string, strlen(uri_string));
        }

        if (cJSON_IsString(username)) {
            char *username_string = cJSON_GetStringValue(username);
            m_mqtt.username(username_string, strlen(username_string));
        }

        if (cJSON_IsString(password)) {
            char *password_string = cJSON_GetStringValue(password);
            size_t password_string_len = strlen(password_string);
            if (password_string_len >= 8) {
                m_mqtt.password(password_string, password_string_len);
            }
        }

        if (cJSON_IsBool(enabled)) {
            m_mqtt.enabled(cJSON_IsTrue(enabled));
        }
    }

    cJSON *canbus = cJSON_GetObjectItem(root, "canbus");
    if (cJSON_IsObject(canbus)) {
        cJSON *bitrate = cJSON_GetObjectItem(canbus, "baudrate");

        if (cJSON_IsString(bitrate)) {
            char *bitrate_string = cJSON_GetStringValue(bitrate);
            m_can.bitrate(ICAN::bitrate(bitrate_string));
        }
    }

    ESP_ERROR_CHECK(nvs_commit(nvs_handle));
    nvs_close(nvs_handle);
}

void Command::relais_rollershutter(char *cmd, cJSON *root) {
    bool relais = (strcmp(cmd, "relais") == 0);
    bool rollershutter = (strcmp(cmd, "rollershutter") == 0);
    if (!rollershutter && !relais) {
        return;
    }

    cJSON *num_json = cJSON_GetObjectItem(root, "num");
    cJSON *state_json = cJSON_GetObjectItem(root, "state");
    cJSON *time_json = cJSON_GetObjectItem(root, "time");
    cJSON *bank_json = cJSON_GetObjectItem(root, "bank");

    if (!cJSON_IsNumber(num_json) || !cJSON_IsNumber(state_json) || !cJSON_IsNumber(time_json) ||
        !cJSON_IsNumber(bank_json)) {
        return;
    }

    union {
        ICAN::RELAIS_MSG_t relais_msg;
        uint8_t data[8];
    };
    relais_msg.number = static_cast<uint8_t>(cJSON_GetNumberValue(num_json));
    relais_msg.state = static_cast<uint8_t>(cJSON_GetNumberValue(state_json));
    relais_msg.time = static_cast<uint32_t>(cJSON_GetNumberValue(time_json));
    relais_msg.bank = static_cast<uint32_t>(cJSON_GetNumberValue(bank_json));
    if (relais) {
        send_can_command(root, ICAN::MSG_ID_t::RELAIS, &data[0], 8, false);
    } else if (rollershutter) {
        send_can_command(root, ICAN::MSG_ID_t::ROLLERSHUTTER, &data[0], 8, false);
    }
}

void Command::lamps(char *cmd, cJSON *root) {
    if (strcmp(cmd, "lamp") != 0) {
        return;
    }

    cJSON *value_json = cJSON_GetObjectItem(root, "value");
    cJSON *bitmask_json = cJSON_GetObjectItem(root, "bitmask");
    cJSON *bank_json = cJSON_GetObjectItem(root, "bank");

    if (!cJSON_IsNumber(value_json) || !cJSON_IsNumber(bitmask_json) ||
        !cJSON_IsNumber(bank_json)) {
        return;
    }

    union {
        ICAN::LAMP_MSG_t lamp_msg;
        uint8_t data[8];
    };

    lamp_msg.value = static_cast<uint8_t>(cJSON_GetNumberValue(value_json));
    lamp_msg.bitmask = static_cast<uint32_t>(cJSON_GetNumberValue(bitmask_json));
    lamp_msg.bank = static_cast<uint32_t>(cJSON_GetNumberValue(bank_json));
    send_can_command(root, ICAN::MSG_ID_t::LAMP_GROUP, &data[0], 4, false);
}

void Command::mqtt_logging(char *cmd, cJSON *root) {
    if (strcmp(cmd, "mqtt_logging") != 0) {
        return;
    }

    cJSON *enabled_json = cJSON_GetObjectItem(root, "enabled");
    if (cJSON_IsBool(enabled_json)) {
        bool enabled_bool = cJSON_IsTrue(enabled_json);
        m_logging.mqtt_logging(enabled_bool);
    }
}

void Command::save_device(char *cmd, cJSON *root) {
    if (strcmp(cmd, "save") != 0) {
        return;
    }

    cJSON *type_json = cJSON_GetObjectItem(root, "type");
    cJSON *id_json = cJSON_GetObjectItem(root, "id");
    if (cJSON_IsString(type_json)) {
        uint8_t data[2]{0};
        if (cJSON_IsString(id_json)) {
            char *id_string = cJSON_GetStringValue(id_json);
            data[0] = std::stoul(std::string(id_string), nullptr, 16) & 0xFF;
        }

        char *type_string = cJSON_GetStringValue(type_json);
        data[1] = std::stoul(std::string(type_string), nullptr, 16) & 0xFF;

        // send select uid0 first. there is a bypass by sending all 8 bytes zero.
        // we just do this for now.
        uint8_t uid0_data[8]{0};
        send_can_command(root, ICAN::MSG_ID_t::DEVICE_UID0, &uid0_data[0], sizeof(uid0_data),
                         false);
        send_can_command(root, ICAN::MSG_ID_t::DEVICE_ID_TYPE, &data[0], 2, false);
        send_can_command(root, ICAN::MSG_ID_t::DEVICE_ID_TYPE, NULL, 0, true);
    }

    cJSON *baudrate_json = cJSON_GetObjectItem(root, "baudrate");
    if (cJSON_IsString(baudrate_json)) {
        uint8_t bitrate = static_cast<uint8_t>(ICAN::bitrate(cJSON_GetStringValue(baudrate_json)));
        send_can_command(root, ICAN::MSG_ID_t::BAUDRATE, &bitrate, 1, false);
        send_can_command(root, ICAN::MSG_ID_t::BAUDRATE, NULL, 0, true);
    }

    cJSON *hwrev_json = cJSON_GetObjectItem(root, "hwrev");
    if (cJSON_IsNumber(hwrev_json)) {
        uint8_t hwrev = static_cast<uint8_t>(cJSON_GetNumberValue(hwrev_json));
        send_can_command(root, ICAN::MSG_ID_t::HW_REV, &hwrev, 1, false);
        send_can_command(root, ICAN::MSG_ID_t::HW_REV, NULL, 0, true);
    }

    cJSON *legacy_sensor_json = cJSON_GetObjectItem(root, "legacy_sensor");
    if (cJSON_IsNumber(legacy_sensor_json)) {
        uint8_t legacy_sensor = static_cast<uint8_t>(cJSON_GetNumberValue(legacy_sensor_json));
        send_can_command(root, ICAN::MSG_ID_t::SENSOR_LEGACY_MODE, &legacy_sensor, 1, false);
        send_can_command(root, ICAN::MSG_ID_t::SENSOR_LEGACY_MODE, NULL, 0, true);
    }

    cJSON *custom_string_json = cJSON_GetObjectItem(root, "custom_string");
    if (cJSON_IsString(custom_string_json)) {
        char *custom_string_string = cJSON_GetStringValue(custom_string_json);
        uint8_t custom_string[8]{0};
        size_t len = strlen(custom_string_string);
        size_t min_len = std::min(len, sizeof(custom_string));
        for (unsigned int i = 0; i < min_len; i++) {
            custom_string[i] = custom_string_string[i];
        }
        send_can_command(root, ICAN::MSG_ID_t::CUSTOM_STRING, &custom_string[0], min_len, false);
        send_can_command(root, ICAN::MSG_ID_t::CUSTOM_STRING, NULL, 0, true);
    }
    cJSON *rollershutter_mode_json = cJSON_GetObjectItem(root, "rollershutter_mode");
    if (cJSON_IsString(rollershutter_mode_json)) {
        char *rollershutter_mode_string = cJSON_GetStringValue(rollershutter_mode_json);
        uint8_t rollershutter_mode = 1;
        if (strcmp(rollershutter_mode_string, "HARDWARE") == 0) {
            rollershutter_mode = 2;
        }

        send_can_command(root, ICAN::MSG_ID_t::ROLLERSHUTTER_MODE, &rollershutter_mode, 1, false);
        send_can_command(root, ICAN::MSG_ID_t::ROLLERSHUTTER_MODE, NULL, 0, true);
    }
}

// for manually going to update mode for legacy devices (stm32 based). manually restart when
// finished
void Command::legacy_mode(char *cmd, cJSON *root) {
    if (strcmp(cmd, "legacy_mode") == 0) {
        uint8_t update_mode = static_cast<uint8_t>(ICAN::AVAILABLE_t::UPDATE_MODE);
        send_can_command(root, ICAN::MSG_ID_t::RESTART, &update_mode, 1, false);
    }
}

void Command::silence_on(char *cmd, cJSON *root) {
    if (strcmp(cmd, "silence_on") == 0) {
        uint8_t silence = static_cast<uint8_t>(ICAN::SILENCE_t::SILENCE_ON);
        send_can_command(root, ICAN::MSG_ID_t::UPDATE_SILENCE, &silence, 1, false);
    }
}

void Command::silence_off(char *cmd, cJSON *root) {
    if (strcmp(cmd, "silence_off") == 0) {
        uint8_t silence = static_cast<uint8_t>(ICAN::SILENCE_t::SILENCE_OFF);
        send_can_command(root, ICAN::MSG_ID_t::UPDATE_SILENCE, &silence, 1, false);
    }
}

void Command::refresh_device(char *cmd, cJSON *root) {
    if (strcmp(cmd, "refresh") == 0) {
        send_can_command(root, ICAN::MSG_ID_t::REQUEST_PARAMETER, NULL, 0, true);
    }
}

void Command::ping_device(char *cmd, cJSON *root) {
    if (strcmp(cmd, "ping") == 0) {
        send_can_command(root, ICAN::MSG_ID_t::AVAILABLE, NULL, 0, true);
    }
}

void Command::restart_device(char *cmd, cJSON *root) {
    if (strcmp(cmd, "restart") != 0) {
        return;
    }

    char *unit = cJSON_GetStringValue(cJSON_GetObjectItem(root, "unit"));
    if (strcmp(unit, "self") == 0) {
        esp_restart();
    } else {
        // restart to application mode
        uint8_t data[]{static_cast<uint8_t>(ICAN::AVAILABLE_t::APPLICATION)};
        send_can_command(root, ICAN::MSG_ID_t::RESTART, &data[0], 1, false);
    }
}

void Command::prepare_update(char *cmd, cJSON *root) {
    if (strcmp(cmd, "update_prepare") != 0) {
        return;
    }

    cJSON *update_size = cJSON_GetObjectItem(root, "update_size");
    cJSON *update_crc = cJSON_GetObjectItem(root, "update_crc");
    cJSON *update_type = cJSON_GetObjectItem(root, "update_type");

    if (update_crc && update_size && update_type && cJSON_IsString(update_crc) &&
        cJSON_IsString(update_size) && cJSON_IsString(update_type)) {
        ESP_LOGI(TAG, "%s", cJSON_GetStringValue(update_type));
        uint32_t crc = (uint32_t)strtoul(cJSON_GetStringValue(update_crc), NULL, 10);
        uint32_t size = (uint32_t)strtoul(cJSON_GetStringValue(update_size), NULL, 10);
        ESP_LOGI(TAG, "crc %lu size %lu", crc, size);
        if (strcmp(cJSON_GetStringValue(update_type), "self") == 0) {
            m_selfupdate.start();
            m_current_update = &m_selfupdate;
        } else if (strcmp(cJSON_GetStringValue(update_type), "can_by_type") == 0) {
            cJSON *update_id = cJSON_GetObjectItem(root, "update_id");
            if (cJSON_IsString(update_id)) {
                m_current_update = &m_canupdate;
                m_canupdate.by_type_start(cJSON_GetStringValue(update_id), size, crc);
            }
        } else if (strcmp(cJSON_GetStringValue(update_type), "can_by_uid") == 0) {
            cJSON *update_id = cJSON_GetObjectItem(root, "update_id");
            if (cJSON_IsString(update_id)) {
                ESP_LOGI(TAG, "prepared for can_by_id");
                m_current_update = &m_canupdate;
                m_canupdate.by_uid_start(cJSON_GetStringValue(update_id), size, crc);
            }
        } else if (strcmp(cJSON_GetStringValue(update_type), "can_selected") == 0) {
            m_current_update = &m_canupdate;
            // can_update_selected_start(char** uids, uint8_t device_count);
        }
    }
}

void Command::complete_update(char *cmd, cJSON *root) {
    if (strcmp(cmd, "update_complete") != 0) {
        return;
    }

    m_current_update->complete();
    /*
    httpd_resp_set_hdr(req, "Connection", "close");
    httpd_resp_sendstr(req, "Update successfully");
    */
}

IUpdate *Command::current_update() {
    return m_current_update;
}
