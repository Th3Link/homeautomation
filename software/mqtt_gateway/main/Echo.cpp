#include "Echo.hpp"
#include "helper.hpp"

const char *Echo::TAG = "Echo";
Echo::Echo(ICAN &ic) : m_can(ic) {
    // m_mqtt.add_dispatcher(this);
    m_can.add_dispatcher(this);
}

void Echo::init() {}

bool Echo::dispatch(uint32_t identifier, uint8_t *data, unsigned int data_len, bool request) {
    if (ICAN::MSG_COMPARE(identifier, ICAN::MSG_ID_t::ECHO)) {
        m_can.send(identifier, data, data_len, request);
        return true;
    }

    return false;
}
