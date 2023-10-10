#pragma once

#include "ActionLight.hpp"


ActionLight::ActionLight(uint8_t id, uint8_t type, ICAN& ic) : m_id(id), m_type(type),
    m_can(ic)
{
    
}

void ActionLight::trigger(uint8_t trigger_id, uint32_t value)
{
    switch (ActionLight)
    {
        case ActionLight::trigger_id_t::ON:
            
    }
}
