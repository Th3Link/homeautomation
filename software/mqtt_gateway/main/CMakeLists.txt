idf_component_register(SRCS "main.cpp" "esp32-ha-lib/CAN.cpp" "helper.cpp" 
    "Update.cpp" "CANUpdate.cpp" "DeviceList.cpp" "MQTT.cpp" "WiFi.cpp" "Web.cpp" "Logging.cpp"
    "Command.cpp" "http_handler.cpp" "WebCredentials.cpp"
    "BridgeButton.cpp" "BridgeDevice.cpp" "BridgeLamps.cpp" "BridgeRelais.cpp" "BridgeDebug.cpp"
    INCLUDE_DIRS ""
    EMBED_FILES "config_page/index.html" "config_page/minimal.js" "config_page/favicon.png" "config_page/style.css")
