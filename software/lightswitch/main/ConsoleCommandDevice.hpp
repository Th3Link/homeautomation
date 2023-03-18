#pragma once

#include "Console.hpp"
#include "Relais.hpp"
#include "Light.hpp"
#include "Selftest.hpp"

class ConsoleCommandDevice
{
public:
    ConsoleCommandDevice(Console&, Relais&, Light&, Selftest&);
    static ConsoleCommandDevice* console_command;
    static void selftest_start();
    static void selftest_stop();
    static void relais_state(uint8_t bank, uint8_t channel, bool value);
    static void light_set(ICAN::LAMP_MSG_t&);
    Relais& relais();
    Light& light();
    Selftest& selftest();
private:
    Relais& m_relais;
    Light& m_light;
    Selftest& m_selftest;
};

void register_device();
