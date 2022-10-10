#include <message/Atomic.hpp>
#include <mutex>

void message::atomic(bool enable)
{
    static std::mutex mutex;
    if(enable)
    {
        mutex.lock();
    }
    else
    {
        mutex.unlock();
    }
}
