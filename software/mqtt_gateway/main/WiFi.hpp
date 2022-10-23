#pragma once

#include <cstdint>

class WiFi
{
public:
    enum class Mode
    {
        Client,
        AccessPoint
    };
    
    WiFi();
    void init();
    Mode mode();
    const char* mode_str();
    const char* ssid();
    const char* password();
    const char* hostname();
    
private:
    void init_softap(const unsigned char channel);
    bool init_client();
    static const char* TAG;
    Mode m_mode;
    char m_ssid[20];
    char m_password[20];
    char m_hostname[40];
};
