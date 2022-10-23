#pragma once

#include "IUpdate.hpp"
#include <stdint.h>

class Update : public IUpdate
{
public:
    Update();
    void start();
    void abort() override;
    bool data(char* data, uint32_t data_len) override;
    void complete() override;
    void verified();
private:
    static const char* TAG;
};
