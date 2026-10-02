#include <esp_err.h>
#include <esp_log.h>
#include "Selftest.hpp"

const char* Selftest::TAG = "Selftest";

void selftest_task(void *this_ptr)
{
    auto selftest = reinterpret_cast<Selftest*>(this_ptr);
    uint32_t channel = 0;
    
    while (selftest->running())
    {
        for (unsigned int i = 0; i < Relais::MAX_RELAIS_COUNT; i++)
        {
            for (unsigned int j = 0; j < Relais::MAX_CHANNEL_COUNT; j++)
            {
                selftest->relais().state(i,j, false);
            }
            selftest->relais().state(i,channel % Relais::MAX_CHANNEL_COUNT, true);
        }
        
        for (unsigned int i = 0; i < Light::PWM_EXTENDER; i++)
        {
            ICAN::LAMP_MSG_t lamp_msg;
            lamp_msg.bank = i;
            lamp_msg.bitmask = (1 << (channel % Light::PWM_CHANNELS));
            lamp_msg.value = (255 * (((channel / Light::PWM_CHANNELS) + 2) % 5)) / 4;
            
            selftest->light().set_onboard(lamp_msg);
            selftest->light().set_ext(lamp_msg);
        }
        
        channel++;
        vTaskDelay(pdMS_TO_TICKS(500));
    }
    
    // switch everything off
    for (unsigned int i = 0; i < Relais::MAX_RELAIS_COUNT; i++)
    {
        for (unsigned int j = 0; j < Relais::MAX_CHANNEL_COUNT; j++)
        {
            selftest->relais().state(i,j, false);
        }
    }
    for (unsigned int i = 0; i < Light::PWM_EXTENDER; i++)
    {
        ICAN::LAMP_MSG_t lamp_msg;
        lamp_msg.bank = i;
        lamp_msg.bitmask = 0xFFFFFF;
        lamp_msg.value = 0;
        
        selftest->light().set_onboard(lamp_msg);
        selftest->light().set_ext(lamp_msg);
    }
    
    selftest->stop_sem();
    vTaskDelete(NULL);
}

Selftest::Selftest(Relais& r, Light& l) : m_relais(r), m_light(l), m_running(false)
{
    
}

void Selftest::init()
{
    m_stop_sem = xSemaphoreCreateBinary();
}

void Selftest::start()
{
    stop();
    m_running = true;
    xTaskCreate(selftest_task, "selftest_task",  configMINIMAL_STACK_SIZE*4, 
        this, 5, NULL);
}

void Selftest::stop()
{
    if (m_running)
    {
        m_running = false;
        xSemaphoreTake(m_stop_sem, portMAX_DELAY);
    }
}

Relais& Selftest::relais()
{
    return m_relais;
}

Light& Selftest::light()
{
    return m_light;
}

bool Selftest::running()
{
    return m_running;
}

void Selftest::stop_sem()
{
    xSemaphoreGive(m_stop_sem);
}
