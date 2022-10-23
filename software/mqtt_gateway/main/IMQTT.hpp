#pragma once

#include <cstdint>
#include <cstddef>
 
class IMQTTDispatcher
{
public:
    virtual void dispatch(const char* topic, size_t topic_len, const char* data, size_t data_len) = 0;
    virtual void connected() = 0;
};

class IMQTT
{
public:
    virtual void add_dispatcher(IMQTTDispatcher*) = 0;
    virtual void subscribe(const char* topic) = 0;
    virtual void publish(const char* topic, const char* data) = 0;
    virtual void uri(const char*, size_t) = 0;
    virtual void username(const char*, size_t) = 0;
    virtual void password(const char*, size_t) = 0;
    virtual void enabled(bool) = 0;
    virtual const char* uri() = 0;
    virtual const char* username() = 0;
    virtual const char* password() = 0;
    virtual bool enabled() = 0;
};
