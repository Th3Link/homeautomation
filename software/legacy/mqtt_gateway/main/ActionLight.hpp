#pragma once

#include "Action.hpp"
#include "esp32-ha-lib/ICAN.hpp"

class ActionLight : public Action
{
public:
    enum trigger_id_t : uint8_t
    {
        ON, OFF, INC, DEC, MIN, MAX
    };
    ActionLight(uint8_t id, uint8_t type, ICAN& ic);
    void trigger(uint8_t trigger_id, uint32_t value) override;
private:
    uint8_t m_id;
    uint8_t m_type;
    ICAN& m_can;
};
