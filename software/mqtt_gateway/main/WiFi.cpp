/* WiFi station Example

   This example code is in the Public Domain (or CC0 licensed, at your option.)

   Unless required by applicable law or agreed to in writing, this
   software is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
   CONDITIONS OF ANY KIND, either express or implied.
*/
#include "WiFi.hpp"
#include <cstring>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/event_groups.h>
#include <esp_mac.h>
#include <esp_wifi.h>
#include <esp_event.h>
#include <esp_log.h>
#include <esp_system.h>
#include <nvs_flash.h>

#include <lwip/err.h>
#include <lwip/sys.h>

/* The examples use WiFi configuration that you can set via project configuration menu

   If you'd rather not, just change the below entries to strings with
   the config you want - ie #define EXAMPLE_WIFI_SSID "mywifissid"
*/

#define EXAMPLE_ESP_MAXIMUM_RETRY  4

/* FreeRTOS event group to signal when we are connected*/
static EventGroupHandle_t s_wifi_event_group;

/* The event group allows multiple bits for each event, but we only care about two events:
 * - we are connected to the AP with an IP
 * - we failed to connect after the maximum amount of retries */
#define WIFI_CONNECTED_BIT BIT0
#define WIFI_FAIL_BIT      BIT1

static int s_retry_num = 0;


const char* WiFi::TAG = "WiFi";
static const char *TAG = "WiFi";
static void wifi_event_handler(void* arg, esp_event_base_t event_base,
                                    int32_t event_id, void* event_data)
{
    if (event_id == WIFI_EVENT_AP_STACONNECTED) {
        wifi_event_ap_staconnected_t* event = (wifi_event_ap_staconnected_t*) event_data;
        ESP_LOGI(TAG, "station " MACSTR " join, AID=%d",
                 MAC2STR(event->mac), event->aid);
    } else if (event_id == WIFI_EVENT_AP_STADISCONNECTED) {
        wifi_event_ap_stadisconnected_t* event = (wifi_event_ap_stadisconnected_t*) event_data;
        ESP_LOGI(TAG, "station " MACSTR " leave, AID=%d",
                 MAC2STR(event->mac), event->aid);
    } else if (event_id == WIFI_EVENT_STA_START) {
        esp_wifi_connect();
    } else if (event_id == WIFI_EVENT_STA_DISCONNECTED) {
        if (s_retry_num < EXAMPLE_ESP_MAXIMUM_RETRY) {
            esp_wifi_connect();
            s_retry_num++;
            ESP_LOGI(TAG, "retry to connect to the AP");
        } else {
            xEventGroupSetBits(s_wifi_event_group, WIFI_FAIL_BIT);
        }
        ESP_LOGI(TAG,"connect to the AP fail");
        
        vTaskDelay(pdMS_TO_TICKS(4000));
        ESP_LOGI(TAG,"Try new round to connect to AP");
        s_retry_num = 0;
        esp_restart();
    } else if (event_id == IP_EVENT_STA_GOT_IP) {
        ip_event_got_ip_t* event = (ip_event_got_ip_t*) event_data;
        ESP_LOGI(TAG, "got ip:" IPSTR, IP2STR(&event->ip_info.ip));
        s_retry_num = 0;
        xEventGroupSetBits(s_wifi_event_group, WIFI_CONNECTED_BIT);
    }
}

WiFi::WiFi() : m_mode(Mode::AccessPoint), m_ssid({0}), m_password({0}), m_hostname({0})
{   

}

void WiFi::init()
{
    read_nvs();
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    
    // try to connect to configured access point but use own access point if this fails
    
    if (m_mode == Mode::Client)
    {
        if (!init_client())
        {
            strcat(m_ssid, "_ap");
            init_softap(10);
            m_mode = Mode::AccessPoint;
        }
    }
    else
    {
        init_softap(10);
    }
}

bool WiFi::init_client()
{
    s_wifi_event_group = xEventGroupCreate();
    esp_netif_t* netif = esp_netif_create_default_wifi_sta();
    esp_netif_set_hostname(netif, m_hostname);
    
    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&cfg));

    esp_event_handler_instance_t instance_any_id;
    esp_event_handler_instance_t instance_got_ip;
    ESP_ERROR_CHECK(esp_event_handler_instance_register(WIFI_EVENT,
                                                        ESP_EVENT_ANY_ID,
                                                        &wifi_event_handler,
                                                        NULL,
                                                        &instance_any_id));
    ESP_ERROR_CHECK(esp_event_handler_instance_register(IP_EVENT,
                                                        IP_EVENT_STA_GOT_IP,
                                                        &wifi_event_handler,
                                                        NULL,
                                                        &instance_got_ip));

    wifi_config_t wifi_config {};
    memset(&wifi_config, 0, sizeof(wifi_config));
    strcpy(reinterpret_cast<char*>(&wifi_config.sta.ssid[0]), m_ssid);
    strcpy(reinterpret_cast<char*>(&wifi_config.sta.password[0]), m_password);
    wifi_config.sta.threshold.authmode = WIFI_AUTH_WPA_WPA2_PSK;
    //wifi_config.sta.threshold.authmode = WIFI_AUTH_WPA2_WPA3_PSK;
    wifi_config.sta.sae_pwe_h2e = WPA3_SAE_PWE_BOTH;

    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_STA) );
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_STA, &wifi_config) );
    ESP_ERROR_CHECK(esp_wifi_start() );

    ESP_LOGI(TAG, "wifi_init_sta finished.");

    /* Waiting until either the connection is established (WIFI_CONNECTED_BIT) or connection failed for the maximum
     * number of re-tries (WIFI_FAIL_BIT). The bits are set by event_handler() (see above) */
    EventBits_t bits = xEventGroupWaitBits(s_wifi_event_group,
            WIFI_CONNECTED_BIT | WIFI_FAIL_BIT,
            pdFALSE,
            pdFALSE,
            portMAX_DELAY);

    /* xEventGroupWaitBits() returns the bits before the call returned, hence we can test which event actually
     * happened. */
    if (bits & WIFI_CONNECTED_BIT) {
        ESP_LOGI(TAG, "connected to ap SSID:%s password:%s",
                 m_ssid, m_password);
        return true;
    } else if (bits & WIFI_FAIL_BIT) {
        ESP_LOGI(TAG, "Failed to connect to SSID:%s, password:%s",
                 m_ssid, m_password);
        esp_wifi_stop(); 
        return false;
    } else {
        ESP_LOGE(TAG, "UNEXPECTED EVENT");
        esp_wifi_stop();
        return false;
    }
}

void WiFi::init_softap(const unsigned char channel)
{
    esp_netif_t* netif = esp_netif_create_default_wifi_ap();
    esp_netif_set_hostname(netif, m_hostname);
    wifi_init_config_t cfg = WIFI_INIT_CONFIG_DEFAULT();
    ESP_ERROR_CHECK(esp_wifi_init(&cfg));

    ESP_ERROR_CHECK(esp_event_handler_instance_register(WIFI_EVENT,
                                                        ESP_EVENT_ANY_ID,
                                                        &wifi_event_handler,
                                                        NULL,
                                                        NULL));
    
    wifi_config_t wifi_config {};
    memset(&wifi_config, 0, sizeof(wifi_config));
    strcpy(reinterpret_cast<char*>(&wifi_config.ap.ssid[0]), m_ssid);
    wifi_config.ap.channel = channel;
    strcpy(reinterpret_cast<char*>(&wifi_config.ap.password[0]), m_password);
    wifi_config.ap.max_connection = 2;
    wifi_config.ap.authmode = WIFI_AUTH_WPA_WPA2_PSK;
    wifi_config.ap.pmf_cfg.required = false;
    
    if (strlen(m_password) == 0) {
        wifi_config.ap.authmode = WIFI_AUTH_OPEN;
    }

    ESP_ERROR_CHECK(esp_wifi_set_mode(WIFI_MODE_AP));
    ESP_ERROR_CHECK(esp_wifi_set_config(WIFI_IF_AP, &wifi_config));
    ESP_ERROR_CHECK(esp_wifi_start());

    ESP_LOGI(TAG, "wifi_init_softap finished. SSID:%s password:%s channel:%d",
             m_ssid, m_password, channel);
}

void WiFi::read_nvs()
{
    char wifi_mode[20];
    size_t ssid_len = sizeof(m_ssid);
    size_t pw_len = sizeof(m_password);
    size_t wifi_mode_len = sizeof(m_mode);
    size_t hostname_len = sizeof(m_hostname);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    
    if (nvs_get_str(nvs_handle, "wifi_mode", &wifi_mode[0], &wifi_mode_len) != ESP_OK)
    {
        wifi_mode_len = sizeof(wifi_mode);
        nvs_set_str(nvs_handle, "wifi_mode", "ap");
        nvs_get_str(nvs_handle, "wifi_mode", &wifi_mode[0], &wifi_mode_len);
    }
    
    if (strcmp(wifi_mode, "client"))
    {
        m_mode = Mode::Client;
    }
    else
    {
        m_mode = Mode::AccessPoint;
    }
    
    if (nvs_get_str(nvs_handle, "wifi_ssid", &m_ssid[0], &ssid_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "wifi_ssid", "CAN2MQTTSETUP");
        nvs_get_str(nvs_handle, "wifi_ssid", &m_ssid[0], &ssid_len);
        m_mode = Mode::AccessPoint;
    }
    
    if (nvs_get_str(nvs_handle, "wifi_pw", &m_password[0], &pw_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "wifi_pw", "Can2MqttPass");
        nvs_get_str(nvs_handle, "wifi_pw", &m_password[0], &pw_len);
        m_mode = Mode::AccessPoint;
    }

    ESP_LOGI(WiFi::TAG, "WIFI Setup from NVS:");
    ESP_LOGI(WiFi::TAG, "\t SSID: %s",m_ssid);
    ESP_LOGI(WiFi::TAG, "\t PW: %s",m_password);
    ESP_LOGI(WiFi::TAG, "\t Mode: %s",wifi_mode);
    
    if (strlen(m_password) < 8)
    {
        ESP_LOGI(WiFi::TAG, "WIFI Password too short");
        strcpy(m_ssid, "CAN2MQTTSETUP");
        strcpy(m_password, "Can2MqttPass");
        m_mode = Mode::AccessPoint;
    }
    
    esp_err_t hostname_err = nvs_get_str(nvs_handle, "hostname", &m_hostname[0], &hostname_len);
    if (hostname_err != ESP_OK || (strcmp(&m_hostname[0], "") == 0))
    {
        nvs_set_str(nvs_handle, "hostname", "CAN2MQTTBridge");
        nvs_get_str(nvs_handle, "hostname", &m_hostname[0], &hostname_len);
    }
    
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
}

WiFi::Mode WiFi::mode()
{
    return m_mode;
}

const char* WiFi::ssid()
{
    return &m_ssid[0];
}

const char* WiFi::password()
{
    return &m_password[0];
}

const char* WiFi::hostname()
{
    return &m_hostname[0];
}

const char* WiFi::mode_str()
{
    if (m_mode == Mode::Client)
    {
        return "client";
    }
    
    return "ap";
}

void WiFi::mode_str(const char* m)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "wifi_mode", m);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    read_nvs();
}

void WiFi::ssid(const char* s)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "wifi_ssid", s);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    read_nvs();
}

void WiFi::password(const char* p)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "wifi_pw", p);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    read_nvs();
}

void WiFi::hostname(const char* h)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "hostname", h);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    read_nvs();
}
