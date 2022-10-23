#include "Logging.hpp"

const char* Logging::TAG = "Logging";

Logging::Logging(IMQTT& im) : m_mqtt(im)
{
    
}
