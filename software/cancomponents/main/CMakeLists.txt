idf_component_register(SRCS "main.cpp" "esp32-ha-lib/Button.cpp" "esp32-ha-lib/CAN.cpp" 
    "PinConfig.cpp" "ConsoleCommandDevice.cpp" "Selftest.cpp"
    "esp32-ha-lib/THSensor.cpp" "Relais.cpp" "esp32-ha-lib/Console.cpp" "esp32-ha-lib/cmd_nvs.c" "esp32-ha-lib/cmd_system.c" 
    "esp32-ha-lib/Device.cpp" "esp32-ha-lib/Update.cpp" "ExtensionBoard.cpp" "Light.cpp" 
    "esp32-ha-lib/PresenceSensor.cpp" "esp32-ha-lib/EEPROM.cpp" 
    "esp32-ha-lib/AmbientLightSensor.cpp" "esp32-ha-lib/BME680.cpp" 
    "esp32-ha-lib/I2C.cpp" "esp32-ha-lib/Nightlight.cpp" "esp32-ha-lib/eeprom.c"
    INCLUDE_DIRS "" "message/include")
