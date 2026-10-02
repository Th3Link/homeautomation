#pragma once

#include <cstdint>

class LAN
{
public:   
    LAN();
    void init();
    const char* hostname();
    void hostname(const char*);
    void read_nvs();
private:
    static const char* TAG;
    char m_hostname[40];
};
