#pragma once

#include <cstdint>
#include "WiFi.hpp"
#include "LAN.hpp"

class Network
{
public:
    
    Network();
    void init(WiFi&, LAN&);
private:
    static const char* TAG;
};
