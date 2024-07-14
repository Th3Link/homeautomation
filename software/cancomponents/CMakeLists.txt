# The following lines of boilerplate have to be in your project's
# CMakeLists in this exact order for cmake to work correctly
cmake_minimum_required(VERSION 3.5)
set(EXTRA_COMPONENT_DIRS components/esp-idf-lib/components)
set(CMAKE_CXX_STANDARD 20)
set(EXCLUDE_COMPONENTS "mbedtls" "wpa_supplicant" "wifi" "bt" "esp_wifi" "esp_event" "esp_netif" "lwip" "mdns" "esp_mesh" "esp_smartconfig" "esp_now" "esp_eth" "esp_pm")
set(COMPONENTS
    main
    driver
    i2cdev
    ds18x20
    dht
    hal
    freertos
    log
    newlib
    onewire
    nvs_flash
    button
    pca9685
    pca9557
    veml7700
    pcf8574
    console
    BSEC2
    bme680
    esp_phy
)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(cancomponents)
set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} -Wl,--gc-sections")
