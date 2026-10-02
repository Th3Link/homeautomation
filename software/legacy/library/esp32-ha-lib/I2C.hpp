#pragma once


class I2C
{
public:
    I2C();
    void init();
    static const char* TAG;
private:
    bool is_initialized;
};
