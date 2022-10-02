/* WiFi station Example

   This example code is in the Public Domain (or CC0 licensed, at your option.)

   Unless required by applicable law or agreed to in writing, this
   software is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
   CONDITIONS OF ANY KIND, either express or implied.
*/
#include "wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"
#include <rom/md5_hash.h>
#include <WiFi.h>

/* The examples use WiFi configuration that you can set via project configuration menu

   If you'd rather not, just change the below entries to strings with
   the config you want - ie #define EXAMPLE_WIFI_SSID "mywifissid"
*/

#define EXAMPLE_ESP_MAXIMUM_RETRY  4


void wifi_init_softap(const char* ssid, const char* password)
{
    WiFi.softAP(ssid, password);
    WiFi.softAPIP();
}

bool wifi_init(const char* ssid, const char* password)
{
    WiFi.mode(WIFI_STA);
    WiFi.begin(ssid, password);

    delay(5000);
    return (WiFi.status() == WL_CONNECTED);
}
