#pragma once

#include "Echo.hpp"
#include "esp32-ha-lib/ICAN.hpp"

class Echo : public ICANDispatcher {
  public:
    Echo(ICAN &);
    void init();
    bool dispatch(uint32_t identifier, uint8_t *data, unsigned int data_len, bool request) override;

  private:
    ICAN &m_can;
    static const char *TAG;
};
