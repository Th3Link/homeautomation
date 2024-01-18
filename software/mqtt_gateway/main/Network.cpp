/* WiFi station Example

   This example code is in the Public Domain (or CC0 licensed, at your option.)

   Unless required by applicable law or agreed to in writing, this
   software is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
   CONDITIONS OF ANY KIND, either express or implied.
*/
#include "Network.hpp"
#include "WiFi.hpp"
#include "LAN.hpp"
#include <cstring>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/event_groups.h>
#include <esp_mac.h>
#include <esp_event.h>
#include <esp_log.h>
#include <esp_system.h>
#include <nvs_flash.h>
#include <esp_netif.h>

#include <lwip/err.h>
#include <lwip/sys.h>

static int s_retry_num = 0;


const char* Network::TAG = "Network";
static const char *TAG = "Network";

Network::Network()
{   

}

void Network::init(WiFi& wifi, LAN& lan)
{
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    
    lan.init();
    wifi.init();
}
