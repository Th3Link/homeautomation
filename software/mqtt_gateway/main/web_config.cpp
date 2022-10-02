#include "web_config.h"

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/semphr.h"

#include <AsyncTCP.h>
#include <ESPAsyncWebServer.h>
#include <AsyncElegantOTA.h>

AsyncWebServer server(80);

void web_config_init()
{
server.on("/", HTTP_GET, [](AsyncWebServerRequest *request) {
    request->send(200, "text/plain", "Hi! I am ESP32. 2222222");
  });

  AsyncElegantOTA.begin(&server);    // Start ElegantOTA
  server.begin();
}

