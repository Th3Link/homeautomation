#pragma once

#include <freertos/FreeRTOS.h>
#include <freertos/semphr.h>
#include "Relais.hpp"
#include "Light.hpp"

class Selftest
{
public:
    Selftest(Relais&, Light&);
    void init();
    void start();
    void stop();
    void stop_sem();
    Relais& relais();
    Light& light();
    bool running();
    static const char* TAG;
private:
    Relais& m_relais;
    Light& m_light;
    bool m_running;
    SemaphoreHandle_t m_stop_sem;
};
