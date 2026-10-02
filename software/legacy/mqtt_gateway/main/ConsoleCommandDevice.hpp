#pragma once

#include "esp32-ha-lib/Console.hpp"
#include "esp32-ha-lib/cmd_device.h"

class ConsoleCommandDevice
{
public:
    ConsoleCommandDevice(Console&);
    static ConsoleCommandDevice* console_command;
   
private:

};
