/* CAN Network Slave Example

   This example code is in the Public Domain (or CC0 licensed, at your option.)

   Unless required by applicable law or agreed to in writing, this
   software is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
   CONDITIONS OF ANY KIND, either express or implied.
*/

/*
 * The following example demonstrates a slave node in a CAN network. The slave
 * node is responsible for sending data messages to the master. The example will
 * execute multiple iterations, with each iteration the slave node will do the
 * following:
 * 1) Start the CAN driver
 * 2) Listen for ping messages from master, and send ping response
 * 3) Listen for start command from master
 * 4) Send data messages to master and listen for stop command
 * 5) Send stop response to master
 * 6) Stop the CAN driver
 */

#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"
#include "esp_err.h"
#include "esp_log.h"
#include <driver/gpio.h>
#include <driver/twai.h>
#include "mqtt_client.h"
#include "helper.h"
#include "wifi.h"
#include "lan.h"
#include "web_config.h"
#include "nvs_flash.h"
#include "update.h"

/* --------------------- Definitions and static variables ------------------ */
//Example Configuration
#define DATA_PERIOD_MS                  50
#define NO_OF_ITERS                     3
#define ITER_DELAY_MS                   1000
#define RX_TASK_PRIO                    8       //Receiving task priority
#define TX_GPIO_NUM                     14
#define RX_GPIO_NUM                     13
#define LED1_GPIO_NUM                    32
#define LED2_GPIO_NUM                    33
#define EXAMPLE_TAG                     "CAN2MQTT"

static const char *TAG = "main";

static const twai_general_config_t g_config = TWAI_GENERAL_CONFIG_DEFAULT(TX_GPIO_NUM, RX_GPIO_NUM, TWAI_MODE_NO_ACK);
static const twai_timing_config_t t_config_22_222 = {.brp = 200, .tseg_1 = 11, .tseg_2 = 6, .sjw = 1, .triple_sampling = false};
static const twai_timing_config_t t_config_25 = TWAI_TIMING_CONFIG_25KBITS();
static const twai_timing_config_t t_config_50 = TWAI_TIMING_CONFIG_50KBITS();
static const twai_timing_config_t t_config_100 = TWAI_TIMING_CONFIG_100KBITS();
static const twai_filter_config_t f_config = TWAI_FILTER_CONFIG_ACCEPT_ALL();

static SemaphoreHandle_t network_init_sem;
static SemaphoreHandle_t receive_task_sem;
static SemaphoreHandle_t shutdown_sem;

static esp_mqtt_client_handle_t client;

/* --------------------------- Tasks and Functions -------------------------- */

static void can_receive_task(void *arg)
{
    //Listen 
    xSemaphoreTake(receive_task_sem, portMAX_DELAY);
    twai_message_t rx_msg;
    printf("Start CAN receive\n");
    while (1)
    {
        if (twai_receive(&rx_msg, portMAX_DELAY) == ESP_OK)
        {
            can_dispatch(rx_msg.identifier, rx_msg.data, rx_msg.data_length_code);
        }
    }
    vTaskDelete(NULL);
}

void mqtt_publish(const char* topic, const char* data)
{
    esp_mqtt_client_publish(client, 
        topic, 
        data, 0, 0, 0);
}

void can_send(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    twai_message_t tx_msg;
    tx_msg.rtr = (request ? 1 : 0);
    tx_msg.ss = 0;
    tx_msg.self = 0;
    tx_msg.extd = 1;
    tx_msg.identifier = identifier;
    tx_msg.data_length_code = data_len;
    memcpy(tx_msg.data, data, data_len);
    twai_transmit(&tx_msg, portMAX_DELAY);
}

void mqtt_subscribe(const char* topic)
{
    esp_mqtt_client_subscribe(client, topic, 0);
}

static esp_err_t mqtt_event_handler_cb(esp_mqtt_event_handle_t event)
{
    esp_mqtt_client_handle_t client = event->client;
    int msg_id;

    // your_context_t *context = event->context;
    switch (event->event_id) {
        case MQTT_EVENT_CONNECTED:
            ESP_LOGI(TAG, "MQTT_EVENT_CONNECTED");
            
            requestSubscribe();
            
            break;
        case MQTT_EVENT_DISCONNECTED:
            ESP_LOGI(TAG, "MQTT_EVENT_DISCONNECTED");
            break;

        case MQTT_EVENT_SUBSCRIBED:
            ESP_LOGI(TAG, "MQTT_EVENT_SUBSCRIBED, msg_id=%d", event->msg_id);
            msg_id = esp_mqtt_client_publish(client, "/topic/qos0", "data", 0, 0, 0);
            ESP_LOGI(TAG, "sent publish successful, msg_id=%d", msg_id);
            break;
        case MQTT_EVENT_UNSUBSCRIBED:
            ESP_LOGI(TAG, "MQTT_EVENT_UNSUBSCRIBED, msg_id=%d", event->msg_id);
            break;
        case MQTT_EVENT_PUBLISHED:
            ESP_LOGI(TAG, "MQTT_EVENT_PUBLISHED, msg_id=%d", event->msg_id);
            break;
        case MQTT_EVENT_DATA:
        {
            ESP_LOGI(TAG, "MQTT_EVENT_DATA");
            printf("TOPIC=%.*s\r\n", event->topic_len, event->topic);
            printf("DATA=%.*s\r\n", event->data_len, event->data);
            printf("ID=%d\r\n", event->msg_id);
            
            mqtt_dispatch(event->topic, event->topic_len, event->data, event->data_len);
            
            break;
        }
        case MQTT_EVENT_ERROR:
            ESP_LOGI(TAG, "MQTT_EVENT_ERROR");
            break;
        default:
            ESP_LOGI(TAG, "Other event id:%d", event->event_id);
            break;
    }
    return ESP_OK;
}

static void mqtt_event_handler(void *handler_args, esp_event_base_t base, int32_t event_id, void *event_data) {
    ESP_LOGD(TAG, "Event dispatched from event loop base=%s, event_id=%d", base, (int)event_id);
    mqtt_event_handler_cb(event_data);
}



void app_main()
{
    //Add short delay to allow master it to initialize first
    for (int i = 3; i > 0; i--) {
        printf("Slave starting in %d\n", i);
        vTaskDelay(pdMS_TO_TICKS(1000));
    }
    
    //Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
      ESP_ERROR_CHECK(nvs_flash_erase());
      ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);

    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    
    char ssid[20] = {0};
    char password[20] = {0};
    char wifi_mode[20] = {0};
    char hostname[40] = {0};
    char mqtt_uri[60] = {0};
    char mqtt_username[60] = {0};
    char mqtt_password[60] = {0};
    size_t ssid_len = sizeof(ssid);
    size_t pw_len = sizeof(password);
    size_t wifi_mode_len = sizeof(wifi_mode);
    size_t hostname_len = sizeof(hostname);
    size_t mqtt_uri_len = sizeof(mqtt_uri);
    size_t mqtt_username_len = sizeof(mqtt_username);
    size_t mqtt_password_len = sizeof(mqtt_password);
       
    esp_err_t ssid_err = nvs_get_str(nvs_handle, "wifi_ssid", &ssid[0], &ssid_len);
    if (ssid_err != ESP_OK)
    {
        nvs_set_str(nvs_handle, "wifi_ssid", "CAN2MQTTSETUP");
        nvs_get_str(nvs_handle, "wifi_ssid", &ssid[0], &ssid_len);
    }
    
    esp_err_t pw_err = nvs_get_str(nvs_handle, "wifi_pw", &password[0], &pw_len);
    if (pw_err != ESP_OK)
    {
        nvs_set_str(nvs_handle, "wifi_pw", "Can2MqttPass");
        nvs_get_str(nvs_handle, "wifi_pw", &password[0], &pw_len);
    }
       
    if (nvs_get_str(nvs_handle, "wifi_mode", &wifi_mode[0], &wifi_mode_len) != ESP_OK)
    {
        wifi_mode_len = sizeof(wifi_mode);
        nvs_set_str(nvs_handle, "wifi_mode", "ap");
        nvs_get_str(nvs_handle, "wifi_mode", &wifi_mode[0], &wifi_mode_len);
    }
    
    if (strlen(password) < 8)
    {
        ESP_LOGI(EXAMPLE_TAG, "WIFI Password too short");
        strcpy(ssid, "Can2MqttPass");
        strcpy(password, "CAN2MQTTSETUP");
        strcpy(wifi_mode, "ap");
    }
    
    ESP_LOGI(EXAMPLE_TAG, "WIFI Setup from NVS:");
    ESP_LOGI(EXAMPLE_TAG, "\t SSID: %s",ssid);
    ESP_LOGI(EXAMPLE_TAG, "\t PW: %s",password);
    ESP_LOGI(EXAMPLE_TAG, "\t Mode: %s",wifi_mode);
    
    if (nvs_get_str(nvs_handle, "hostname", &hostname[0], &hostname_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "hostname", "CAN2MQTTBridge");
    }
    
    uint8_t mqtt_enabled = 0;
    if (nvs_get_u8(nvs_handle, "mqtt_enabled", &mqtt_enabled) != ESP_OK)
    {
        nvs_set_u8(nvs_handle, "mqtt_enabled", mqtt_enabled);
    }
    
    if (nvs_get_str(nvs_handle, "mqtt_uri", &mqtt_uri[0], &mqtt_uri_len) != ESP_OK)
    {
        mqtt_enabled = 0;
        nvs_set_str(nvs_handle, "mqtt_uri", "mqtt://IP:1883");
    }

    if (nvs_get_str(nvs_handle, "mqtt_user", &mqtt_username[0], &mqtt_username_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "mqtt_user", "");
    }
    
    if (nvs_get_str(nvs_handle, "mqtt_pw", &mqtt_password[0], &mqtt_password_len) != ESP_OK)
    {
        nvs_set_str(nvs_handle, "mqtt_pw", "");
    }
    uint8_t can_bitrate = 0;
    nvs_get_u8(nvs_handle, "can_bitrate", &can_bitrate);
    
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    
    //Create semaphores and tasks
    receive_task_sem  = xSemaphoreCreateBinary();
    shutdown_sem  = xSemaphoreCreateBinary();
    network_init_sem  = xSemaphoreCreateBinary();

    xTaskCreatePinnedToCore(can_receive_task, "CAN_rx", 4096, NULL, RX_TASK_PRIO, NULL, tskNO_AFFINITY);

    // try to connect to configured access point but use own access point if this fails
    bool wifi_ok = false;
    wifi_init();
    if ((ssid_err == ESP_OK) && (pw_err == ESP_OK) && (strcmp(wifi_mode, "ap") != 0))
    {
        printf("wifi_init\n");
        wifi_ok = wifi_init_client(ssid, password, hostname);
    }
    if (!wifi_ok)
    {
        strcat(ssid, "_ap");
    }
    if (!wifi_ok || (strcmp(wifi_mode, "ap") == 0))
    {
        printf("wifi_init_softap\n");
        wifi_init_softap(ssid, password, 10, hostname);
    }
    else
    {
        //Install CAN driver, trigger tasks to start
        const twai_timing_config_t* t_config = &t_config_50;
        if (can_bitrate == 1)
        {
            t_config = &t_config_22_222;
        }
        else if (can_bitrate == 2)
        {
            t_config = &t_config_25;
        }
        else if (can_bitrate == 4)
        {
            t_config = &t_config_100;
        }
        
        ESP_ERROR_CHECK(twai_driver_install(&g_config, t_config, &f_config));
        ESP_LOGI(EXAMPLE_TAG, "Driver installed");
        
        twai_start();
        
        //lan_init(&network_init_sem);
           
        xSemaphoreTake(network_init_sem, pdMS_TO_TICKS(10000));
        printf("mqtt_start_task\n");
        esp_mqtt_client_config_t mqtt_cfg = {
            .broker.address.uri = mqtt_uri,
        };

        if (strcmp(mqtt_username, "") != 0)
        {
            mqtt_cfg.credentials.username = mqtt_password;
        }
        
        if (strcmp(mqtt_password, "") != 0)
        {
            mqtt_cfg.credentials.authentication.password = mqtt_password;
        }
        
        client = esp_mqtt_client_init(&mqtt_cfg);
        esp_mqtt_client_register_event(client, ESP_EVENT_ANY_ID, mqtt_event_handler, client);
        esp_mqtt_client_start(client);   

        xSemaphoreGive(receive_task_sem);              //Start Control task
    }

    web_config_init();
    update_verified();
    
    xSemaphoreTake(shutdown_sem, portMAX_DELAY);    //Wait for tasks to complete

    twai_stop();
    //Uninstall CAN driver
    ESP_ERROR_CHECK(twai_driver_uninstall());
    ESP_LOGI(EXAMPLE_TAG, "Driver uninstalled");

    //Cleanup
    vSemaphoreDelete(receive_task_sem);
    vSemaphoreDelete(shutdown_sem);
}
