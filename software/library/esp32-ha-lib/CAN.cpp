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
    ESP_LOGI(CAN::TAG, "Start CAN receive");
    
    twai_reconfigure_alerts(TWAI_ALERT_RX_QUEUE_FULL | TWAI_ALERT_ARB_LOST | TWAI_ALERT_BUS_ERROR, NULL);
    
    // Send start message
    uint8_t data[1] {static_cast<uint8_t>(ICAN::AVAILABLE_t::APPLICATION)};
    can->send(ICAN::MSG_ID_t::AVAILABLE, data, sizeof(data), false);
    
    while (!can->shutdown_request())
    {
        if (twai_receive(&rx_msg, pdMS_TO_TICKS(100)) == ESP_OK)
        {
            // types are not compared, they are 
            bool id_match = ICAN::ID_COMPARE(rx_msg.identifier,can->get_id());
            bool type_match = ICAN::TYPE_COMPARE(rx_msg.identifier,can->get_type());
            bool id_zero = ICAN::ID_COMPARE(rx_msg.identifier,0);
            if (!can->enable_filter() || id_zero || (id_match && type_match))
            {
                can->dispatch(rx_msg.identifier, rx_msg.data, 
                    rx_msg.data_length_code, rx_msg.rtr);
            }
        }
        uint32_t alerts;
        twai_read_alerts(&alerts, 0);
        if (alerts) {
            uint8_t data[8] {0};
            data[0] = static_cast<uint8_t>(ICAN::ERROR_t::COMPONENT_CAN);
            data[1] = 0;
            data[4] = alerts & 0xFF;
            data[5] = alerts >> 8 & 0xFF;
            data[6] = alerts >> 16 & 0xFF;
            data[7] = alerts >> 24 & 0xFF;
            can->send(ICAN::MSG_ID_t::DEVICE_ERROR, data, sizeof(data), false);
            //ESP_LOGI(CAN::TAG, "TWAI ALERT %lu", alerts);
        }
    }
    can->shutdown();
    vTaskDelete(NULL);
}

CAN::CAN() : 
    m_enable_filter(false), m_update_silence(false), m_bitrate(ICAN::BITRATE_t::BITRATE_50), m_id(0xFF), 
    m_type(0xFF), m_rx_pin(GPIO_NUM_NC), m_tx_pin(GPIO_NUM_NC), m_shutdown_request(false),
    m_received(0), m_transmitted(0)
{
}

void CAN::init(PinConfig::can_config_t can_config, bool enable_filter)
{
    m_rx_pin = can_config.rx;
    m_tx_pin = can_config.tx;
    m_enable_filter = enable_filter;
    m_shutdown_sem  = xSemaphoreCreateBinary();

    #define RX_TASK_PRIO                    10       //Receiving task priority
    static twai_general_config_t g_config = TWAI_GENERAL_CONFIG_DEFAULT(
        m_tx_pin, m_rx_pin, TWAI_MODE_NORMAL);
    g_config.tx_queue_len = 100;
    g_config.tx_queue_len = 100;
    static const twai_timing_config_t t_config_22_222 = {.brp = 200, .tseg_1 = 11, 
        .tseg_2 = 6, .sjw = 3, .triple_sampling = true};
    static const twai_timing_config_t t_config_25 = TWAI_TIMING_CONFIG_25KBITS();
    static const twai_timing_config_t t_config_50 = TWAI_TIMING_CONFIG_50KBITS();
    static const twai_timing_config_t t_config_100 = TWAI_TIMING_CONFIG_100KBITS();
    
    read_nvs();
    
    //Install CAN driver, trigger tasks to start
    const twai_timing_config_t* t_config = &t_config_50;
    if (m_bitrate == ICAN::BITRATE_t::BITRATE_22_222)
    {
        t_config = &t_config_22_222;
    }
    else if (m_bitrate == ICAN::BITRATE_t::BITRATE_25)
    {
        t_config = &t_config_25;
    }
    else if (m_bitrate == ICAN::BITRATE_t::BITRATE_100)
    {
        t_config = &t_config_100;
    }
    
    g_config.alerts_enabled = TWAI_ALERT_BELOW_ERR_WARN | 
      TWAI_ALERT_RECOVERY_IN_PROGRESS | 
      TWAI_ALERT_RX_QUEUE_FULL | 
      TWAI_ALERT_RX_FIFO_OVERRUN |
      TWAI_ALERT_BUS_ERROR | 
      TWAI_ALERT_ARB_LOST;
    
    twai_filter_config_t f_config = TWAI_FILTER_CONFIG_ACCEPT_ALL();
    ESP_ERROR_CHECK(twai_driver_install(&g_config, t_config, &f_config));
    
    twai_start();
    xTaskCreatePinnedToCore(can_receive_task, "CAN_rx", 4096, this, RX_TASK_PRIO, NULL, tskNO_AFFINITY);
}

void CAN::deinit()
{
    m_shutdown_request = true;
    // wait for receive task for a clean shutdown
    xSemaphoreTake(m_shutdown_sem, portMAX_DELAY);
    //Uninstall CAN driver
    twai_stop();
    ESP_ERROR_CHECK(twai_driver_uninstall());
    ESP_LOGI(TAG, "Driver uninstalled");
}

void CAN::shutdown()
{
    // will be called from the receive task
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

bool CAN::dispatch(uint32_t identifier, uint8_t* data, unsigned int data_len, bool request)
{
    inc_received();
    
    if (GET_MSG(identifier) == MSG_ID_t::UPDATE_SILENCE)
    {
        m_update_silence = data[0];
    }
    
    for (auto* dispatcher : m_dispatcher)
    {
        if (dispatcher->dispatch(identifier, data, data_len, request))
        {
            return true;
        }
    }
    return false;
}

void CAN::send(MSG_ID_t messageId, uint8_t* data, unsigned int data_len, bool request)
{
    if (m_update_silence && messageId > MSG_ID_t::FLASH_VERIFY)
    {
        return;
    }
    send(ICAN::ID_NG_MASK | m_id << 8 | m_type << 16 | static_cast<uint32_t>(messageId),
        data, data_len, request);
}

void CAN::send(uint32_t id, uint8_t* data, unsigned int data_len, bool request)
{
    inc_transmitted();
    twai_message_t tx_msg;
    tx_msg.rtr = (request ? 1 : 0);
    tx_msg.ss = 0;
    tx_msg.self = 0;
    tx_msg.extd = 1;
    tx_msg.identifier = id;
    tx_msg.data_length_code = data_len;
    for (unsigned int i = 0; i < data_len; i++)
    {
        tx_msg.data[i] = data[i];
    }
    
    esp_err_t err = twai_transmit(&tx_msg, portMAX_DELAY);
    if (err != ESP_OK)
    {
        //ESP_LOGE(TAG, "Transmit error %lx", static_cast<uint32_t>(err));
    }
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
    nvs_open("storage", NVS_READONLY, &nvs_handle);
    nvs_get_u8(nvs_handle, "can_bitrate", reinterpret_cast<uint8_t*>(&m_bitrate));
    nvs_get_u8(nvs_handle, "can_id", &m_id);
    nvs_get_u8(nvs_handle, "can_type", &m_type);
    
    nvs_close(nvs_handle);
    
    ESP_LOGI(TAG, "Bitrate: %x", static_cast<uint8_t>(m_bitrate));
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

bool CAN::enable_filter()
{
    return m_enable_filter;
}

void CAN::bitrate(ICAN::BITRATE_t b)
{
    m_bitrate = b;
    nvs_handle_t nvs_handle;
    nvs_open("storage", NVS_READWRITE, &nvs_handle);
    nvs_set_u8(nvs_handle, "can_bitrate", static_cast<uint8_t>(m_bitrate));
    nvs_commit(nvs_handle);
    nvs_close(nvs_handle);
}

ICAN::BITRATE_t CAN::bitrate()
{
    return m_bitrate;
}

void CAN::inc_received()
{
    m_received++;
}

void CAN::inc_transmitted()
{
    m_transmitted++;
}

uint64_t CAN::received()
{
    return m_received;
}

uint64_t CAN::transmitted()
{
    return m_transmitted;
}
