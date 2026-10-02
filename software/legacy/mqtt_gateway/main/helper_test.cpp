#include <cstdint>
#include "helper.h"
#include <string>
#include <iostream>

int main(void)
{
    uint8_t can_data[8] = {0xFF, 0xEE, 0xDD, 0xCC, 0xBB, 0xAA, 0x99, 0x88};
    const char tosplit[] = "canbus/relais/40030402";
    std::cout << "Topic: " << topic_temparature(0x12345678, can_data) << std::endl;
    std::cout << "Data: " << data_temparature(can_data) << std::endl;
    std::cout << "Split 0: " << mqtt_split(tosplit, 0) << std::endl;
    std::cout << "Split 1: " << mqtt_split(tosplit, 1) << std::endl;
    std::cout << "Split 2: " << mqtt_split(tosplit, 2) << std::endl;
    std::cout << "Split 3: " << mqtt_split(tosplit, 3) << std::endl;
    std::cout << "Split 4: " << mqtt_split(tosplit, 4) << std::endl;
}
