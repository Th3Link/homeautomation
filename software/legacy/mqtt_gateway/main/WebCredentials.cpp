#include "WebCredentials.hpp"

#include <nvs_flash.h>
#include <esp_log.h>

const char* WebCredentials::TAG = "WebCredentials";

WebCredentials::WebCredentials()
{
    m_username[0] = 0;
    m_password[0] = 0;
}

void WebCredentials::init()
{
    size_t username_len = sizeof(m_username);
    size_t password_len = sizeof(m_password);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    
    if (nvs_get_str(nvs_handle, "web_username", &m_username[0], &username_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "web_username", "CAN2MQTTSETUP");
        nvs_get_str(nvs_handle, "web_username", &m_username[0], &username_len);
    }
    
    if (nvs_get_str(nvs_handle, "web_password", &m_password[0], &password_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "web_password", "Can2MqttPass");
        nvs_get_str(nvs_handle, "web_password", &m_password[0], &password_len);
    }
    
    ESP_LOGI(TAG, "Username: %s Password: %s", m_username, m_password);
    
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
}

const char* WebCredentials::username()
{
    return &m_username[0];
}

const char* WebCredentials::password()
{
    return &m_password[0];
}

void WebCredentials::username(const char* c)
{
    size_t username_len = sizeof(m_username);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "web_username", c);
    nvs_get_str(nvs_handle, "web_username", &m_username[0], &username_len);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
}

void WebCredentials::password(const char* c)
{
    size_t password_len = sizeof(m_password);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "web_password", c);
    nvs_get_str(nvs_handle, "web_password", &m_password[0], &password_len);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
}
