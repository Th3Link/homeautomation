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
#include <esp_eth.h>
#include <esp_eth_phy.h>
#include <esp_eth_driver.h>
#include <lwip/err.h>
#include <lwip/sys.h>


#define EXAMPLE_ESP_MAXIMUM_RETRY  4

const char* LAN::TAG = "LAN";
static const char *TAG = "LAN";

static void eth_event_handler(void *arg, esp_event_base_t event_base,
                              int32_t event_id, void *event_data)
{
    uint8_t mac_addr[6] = {0};
    /* we can get the ethernet driver handle from event data */
    esp_eth_handle_t eth_handle = *(esp_eth_handle_t *)event_data;

    switch (event_id) {
    case ETHERNET_EVENT_CONNECTED:
        esp_eth_ioctl(eth_handle, ETH_CMD_G_MAC_ADDR, mac_addr);
        ESP_LOGI(TAG, "Ethernet Link Up");
        ESP_LOGI(TAG, "Ethernet HW Addr %02x:%02x:%02x:%02x:%02x:%02x",
                    mac_addr[0], mac_addr[1], mac_addr[2], mac_addr[3], mac_addr[4], mac_addr[5]);
        break;
    case ETHERNET_EVENT_DISCONNECTED:
        ESP_LOGI(TAG, "Ethernet Link Down");
        break;
    case ETHERNET_EVENT_START:
        ESP_LOGI(TAG, "Ethernet Started");
        break;
    case ETHERNET_EVENT_STOP:
        ESP_LOGI(TAG, "Ethernet Stopped");
        break;
    default:
        break;
    }
}

LAN::LAN() : m_hostname({0})
{   

}

void LAN::init()
{   
    read_nvs();
    ESP_ERROR_CHECK(esp_netif_init());
    ESP_ERROR_CHECK(esp_event_loop_create_default());
    
    eth_mac_config_t mac_config = ETH_MAC_DEFAULT_CONFIG();      // apply default MAC configuration
    
    eth_esp32_emac_config_t esp32_emac_config = ETH_ESP32_EMAC_DEFAULT_CONFIG();
    esp32_emac_config.interface = EMAC_DATA_INTERFACE_RMII; // alter EMAC Data Interface
    esp32_emac_config.clock_config.rmii.clock_mode = EMAC_CLK_OUT; // select EMAC REF_CLK mode
    esp32_emac_config.clock_config.rmii.clock_gpio = EMAC_CLK_OUT_180_GPIO; // select GPIO number used to input/output EMAC REF_CLK   
    esp32_emac_config.smi_mdc_gpio_num = 23;   // alter the GPIO used for MDC signal
    esp32_emac_config.smi_mdio_gpio_num = 18; // alter the GPIO used for MDIO signal
    esp_eth_mac_t *mac = esp_eth_mac_new_esp32(&esp32_emac_config, &mac_config);     // create MAC instance

    eth_phy_config_t phy_config = ETH_PHY_DEFAULT_CONFIG();      // apply default PHY configuration
    phy_config.phy_addr = 1;           // alter the PHY address according to your board design
    phy_config.reset_gpio_num = 5; // alter the GPIO used for PHY reset
    esp_eth_phy_t *phy = esp_eth_phy_new_lan87xx(&phy_config);
    
    esp_eth_handle_t eth_handle = NULL;
    esp_eth_config_t config = ETH_DEFAULT_CONFIG(mac, phy);
    ESP_ERROR_CHECK(esp_eth_driver_install(&config, &eth_handle));
    
    esp_event_handler_register(ETH_EVENT, ESP_EVENT_ANY_ID, &eth_event_handler, NULL); // register Ethernet event handler (to deal with user-specific stuff when events like link up/down happened)
}

void LAN::read_nvs()
{
    size_t hostname_len = sizeof(m_hostname);
    
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    
    esp_err_t hostname_err = nvs_get_str(nvs_handle, "hostname", &m_hostname[0], &hostname_len);
    if (hostname_err != ESP_OK || (strcmp(&m_hostname[0], "") == 0))
    {
        nvs_set_str(nvs_handle, "hostname", "CAN2MQTTBridge");
        nvs_get_str(nvs_handle, "hostname", &m_hostname[0], &hostname_len);
    }
    
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
}

const char* LAN::hostname()
{
    return &m_hostname[0];
}

void LAN::hostname(const char* h)
{
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_str(nvs_handle, "hostname", h);
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
    read_nvs();
}
