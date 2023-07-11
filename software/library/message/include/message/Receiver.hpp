#ifndef __RECEIVER_HPP__
#define __RECEIVER_HPP__

#include <chrono>

namespace message
{
    template<typename MSG_T>
    class Message;
    
    class ReceiverQueue
    {
    public:
        virtual void* alloc(std::size_t size) = 0;
        virtual void* alloc(std::size_t size, 
            std::chrono::milliseconds delay, 
            uint16_t** timeoutSize, uint16_t& length, uint32_t id) = 0;
        virtual bool enoughSpace(std::size_t size) = 0;
        virtual bool enoughTimeoutSpace(std::size_t size) = 0;
        virtual std::size_t getLevel() const = 0;
        virtual void incLevel() = 0;
        virtual void cancelTimeouts(uint32_t id) = 0;
    };
    
    template<typename MSG_T>
    class Receiver
    {
    public:
        Receiver(ReceiverQueue& q) : queue(q)
        {
            
        }
        
        virtual void receive(Message<MSG_T>&) = 0;
        ReceiverQueue& queue;
    };
}
#endif //__RECEIVER_HPP__
