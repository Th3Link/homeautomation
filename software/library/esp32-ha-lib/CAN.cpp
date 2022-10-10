#include <cstdint>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>
#include <freertos/semphr.h>
#include <driver/twai.h>
#include <esp_err.h>
#include <esp_log.h>
#include <nvs_flash.h>

#include "CAN.hpp"

const char* CAN::TAG = "CAN";

static void can_receive_task(void *this_ptr)
{
    auto can = reinterpret_cast<CAN*>(this_ptr);
    //Listen
    twai_message_t rx_msg;
    ESP_LOGI(CAN::TAG, "Start CAN receive\n");
    
    // Send start message
    uint8_t data[1] {static_cast<uint8_t>(ICAN::AVAILABLE_t::APPLICATION)};
    can->send(ICAN::MSG_ID_t::AVAILABLE, data, sizeof(data), false);
    
    while (!can->shutdown_request())
    {
        if (twai_receive(&rx_msg, portMAX_DELAY) == ESP_OK)
        {
            if (((rx_msg.identifier & 0xFF00) == (can->get_id() << 8)) ||
                ((rx_msg.identifier & 0xFF00) == 00))
            {
                
            }
            
            for (auto& dispatcher : can->dispatcher())
            {
                dispatcher->dispatch(rx_msg.identifier, rx_msg.data, 
                    rx_msg.data_length_code, rx_msg.rtr);
            }
        }
    }
    can->shutdown();
    vTaskDelete(NULL);
}

CAN::CAN(gpio_num_t rx_pin, gpio_num_t tx_pin) : m_bitrate(0), m_id(0xFF), 
    m_type(0x4), m_rx_pin(rx_pin), m_tx_pin(tx_pin)
{
    m_shutdown_sem  = xSemaphoreCreateBinary();
}

void CAN::init()
{
    #define RX_TASK_PRIO                    10       //Receiving task priority

    static twai_general_config_t g_config = TWAI_GENERAL_CONFIG_DEFAULT(
        m_tx_pin, m_rx_pin, TWAI_MODE_NORMAL);
    static const twai_timing_config_t t_config_22_222 = {.brp = 200, .tseg_1 = 11, 
        .tseg_2 = 6, .sjw = 3, .triple_sampling = true};
    static const twai_timing_config_t t_config_25 = TWAI_TIMING_CONFIG_25KBITS();
    static const twai_timing_config_t t_config_50 = TWAI_TIMING_CONFIG_50KBITS();
    static const twai_timing_config_t t_config_100 = TWAI_TIMING_CONFIG_100KBITS();
    
    read_nvs();
    
    //Install CAN driver, trigger tasks to start
    const twai_timing_config_t* t_config = &t_config_50;
    if (m_bitrate == 1)
    {
        t_config = &t_config_22_222;
    }
    else if (m_bitrate == 2)
    {
        t_config = &t_config_25;
    }
    else if (m_bitrate == 4)
    {
        t_config = &t_config_100;
    }
    
    g_config.rx_queue_len = 200;
    g_config.tx_queue_len = 200;
    g_config.alerts_enabled = TWAI_ALERT_BELOW_ERR_WARN | 
      TWAI_ALERT_RECOVERY_IN_PROGRESS | 
      TWAI_ALERT_RX_QUEUE_FULL | 
      TWAI_ALERT_RX_FIFO_OVERRUN |
      TWAI_ALERT_BUS_ERROR | 
      TWAI_ALERT_ARB_LOST;

    twai_filter_config_t f_config = {
        .acceptance_code = ((m_type << 16) | m_can_ng) << 3, 
        .acceptance_mask = ((0xFF00FFFF & ~m_can_ng) << 3) | 0xFF, 
        .single_filter = true};
    
    ESP_ERROR_CHECK(twai_driver_install(&g_config, t_config, &f_config));
    
    twai_start();
    xTaskCreatePinnedToCore(can_receive_task, "CAN_rx", 4096, this, RX_TASK_PRIO, NULL, tskNO_AFFINITY);
}

void CAN::deinit()
{
    m_shutdown_request = true;
    xSemaphoreTake(m_shutdown_sem, portMAX_DELAY);
    //Uninstall CAN driver
    twai_stop();
    ESP_ERROR_CHECK(twai_driver_uninstall());
    ESP_LOGI(TAG, "Driver uninstalled");
}

void CAN::shutdown()
{
    xSemaphoreGive(m_shutdown_sem); 
}

bool CAN::shutdown_request()
{
    return m_shutdown_request; 
}

void CAN::add_dispatcher(ICANDispatcher* dispatcher)
{
    m_dispatcher.push_back(dispatcher);
}

std::vector<ICANDispatcher*> CAN::dispatcher()
{
    return m_dispatcher;
}

void CAN::send(MSG_ID_t messageId, uint8_t* data, unsigned int data_len, bool request)
{
    twai_message_t tx_msg;
    tx_msg.rtr = (request ? 1 : 0);
    tx_msg.ss = 0;
    tx_msg.self = 0;
    tx_msg.extd = 1;
    tx_msg.identifier = m_can_ng | m_id << 8 | m_type << 16 | static_cast<uint32_t>(messageId);
    tx_msg.data_length_code = data_len;
    for (unsigned int i = 0; i < data_len; i++)
    {
        tx_msg.data[i] = data[i];
    }
    twai_transmit(&tx_msg, portMAX_DELAY);
}

void CAN::read_nvs()
{
    //Initialize NVS
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
      ESP_ERROR_CHECK(nvs_flash_erase());
      ret = nvs_flash_init();
    }
    ESP_ERROR_CHECK(ret);
    
    nvs_handle_t nvs_handle;
    ESP_ERROR_CHECK(nvs_open("storage", NVS_READONLY, &nvs_handle));
    ESP_ERROR_CHECK(nvs_get_u8(nvs_handle, "can_bitrate", &m_bitrate));
    ESP_ERROR_CHECK(nvs_get_u8(nvs_handle, "can_id", &m_id));
    ESP_ERROR_CHECK(nvs_get_u8(nvs_handle, "can_type", &m_type));
    
    nvs_close(nvs_handle);
    
    ESP_LOGI(TAG, "Bitrate: %x", m_bitrate);
    ESP_LOGI(TAG, "CAN ID: %x", m_id);
    ESP_LOGI(TAG, "CAN TYPE: %x", m_type);
}

uint8_t CAN::get_type()
{
    return m_type;
}

uint8_t CAN::get_id()
{
    return m_id;
}
