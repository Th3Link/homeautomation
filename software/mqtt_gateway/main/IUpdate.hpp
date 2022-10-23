#pragma once

#include <stdint.h>

class IUpdate
{
public:
    virtual void abort() = 0;
    virtual bool data(char* data, uint32_t data_len) = 0;
    virtual void complete() = 0;
};
