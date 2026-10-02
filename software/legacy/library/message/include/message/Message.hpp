#ifndef __MESSAGE_HPP__
#define __MESSAGE_HPP__

#include <chrono>
#include <algorithm>
#include "Receiver.hpp"
#include "Atomic.hpp"

namespace message
{
    template<typename MSG_T>
    class Receiver;
    
    enum class Event
    {
        UPDATE,
        INIT,
        CAN_RELAIS_SET,
        CAN_RELAIS_GET,
        STOP_TIME,
        BLOCK_TIME,
        CAN_ROLLERSHUTTER_SET,
        CAN_ROLLERSHUTTER_GET
    };
    
    template<typename MSG_T>
    class Message
    {
    private:
        void* operator new(size_t size, ReceiverQueue& queue)
        {
            return(static_cast<Message<MSG_T>*>(queue.alloc(size)));
        }
        void* operator new(size_t size, ReceiverQueue& queue,
            std::chrono::milliseconds delay,
            uint16_t** timeoutSize, uint16_t& length, uint32_t id)
        {
            return(static_cast<Message<MSG_T>*>(queue.alloc(size, delay,
                timeoutSize, length, id)));
        }
        
        Message(Receiver<MSG_T>& r, Event e, MSG_T&& d) : event(e), 
            receiver(r), data(d)
        {
        }
        
    public:
        Message(Receiver<MSG_T>& r) : receiver(r) {}
        Event event;
        Receiver<MSG_T>& receiver;
    public:
        static void send(ReceiverQueue& q, Receiver<MSG_T>& r, Event e, MSG_T&& data, std::chrono::milliseconds delay, uint32_t id = 0)
        {
            // cancel all other timeouts for this id
            if (id != 0)
            {
                message::atomic(true);
                q.cancelTimeouts(id);
                message::atomic(false);
            }
            
            if (q.enoughTimeoutSpace(sizeof(Message<MSG_T>)))
            {
                message::atomic(true);
                uint16_t* size = 0;
                uint16_t len;
                new(q, delay, &size, len, id) Message<MSG_T>(r, e, std::move(data));
                /* 
                 * assure that the size is written after the data (and the fx pointer)
                 * is set. size is the indirect marker for an active timeout.
                 */
                *size = len;
                message::atomic(false);
            }
        }
        static void send(ReceiverQueue& q,Receiver<MSG_T>& r, Event e, MSG_T&& data)
        {
            if (q.enoughSpace(sizeof(Message<MSG_T>)))
            {
                message::atomic(true);
                new(q) Message<MSG_T>(r, e, std::move(data));
                q.incLevel();
                message::atomic(false);
            }
        }
        void deliver()
        {
            receiver.receive(*this);
        }
        MSG_T data;
    };
}

#endif //__MESSAGE_HPP__
