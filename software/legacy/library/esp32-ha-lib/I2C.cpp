// std
#include <cstdint>

// lib
#include <esp_err.h>
#include <esp_log.h>
#include <i2cdev.h>
// local
#include "I2C.hpp"

const char* I2C::TAG = "I2C";

I2C::I2C() : is_initialized(false)
{
    
}

void I2C::init()
{
    if (!is_initialized)
    {
        i2cdev_init();
        is_initialized = true;
    }
}
