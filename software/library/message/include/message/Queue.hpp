#ifndef __QUEUE_HPP__
#define __QUEUE_HPP__

#include <array>
#include <chrono>
#include "Receiver.hpp"
#include "Atomic.hpp"
#include "Message.hpp"
#include <esp_log.h>

namespace message
{
    template<uint16_t QUEUE_SIZE, uint16_t TIMEOUT_QUEUE_SIZE>
    class Queue : public ReceiverQueue
    {
    public:
        Queue() : queue({0}), timeoutQueue({0}), read(0), write(0), writeTimeout(0), level(0) {}
        
        bool enoughSpace(std::size_t size) override
        {
            auto space = sizeof(uint16_t) + size;
            return (write + space) - read <= QUEUE_SIZE;
        }
        
        bool enoughTimeoutSpace(std::size_t size) override
        {
            for (auto& index : timeoutQueueIndex)
            {
                if (index.size == 0)
                {
                    return (writeTimeout + size) <= timeoutQueue.size();
                }
            }
            return false;
        }
        
        void* alloc(std::size_t size) override
        {
            // calculate size
            auto space = sizeof(uint32_t) + size;
            auto realWrite = write % QUEUE_SIZE;
            
            // check rollover
            if ((realWrite < QUEUE_SIZE) && ((realWrite + space) > QUEUE_SIZE))
            {
                // write 0xFF to the remainder
                // leave it to the read function if only one byte is left
                while (realWrite != 0)
                {
                    auto end = reinterpret_cast<uint8_t*>(&queue[realWrite]);
                    *end = 0xFF;
                    write++;
                    realWrite = write % QUEUE_SIZE;
                }
            }
            
            auto sizeAddress = reinterpret_cast<uint32_t*>(&queue[realWrite]);
            *sizeAddress = size;
            write += sizeof(uint32_t);
            auto address = &queue[write % queue.size()];
            write += size;        
            
            return address;
        }
        
        void* alloc(std::size_t size, std::chrono::milliseconds delay,
            uint16_t** timeoutSize, uint16_t& length, uint32_t id) override
        {
            for (auto& index : timeoutQueueIndex)
            {
                if (index.size == 0)
                {
                    index.address = writeTimeout;
                    index.size = size;
                    index.timeout = 
                        std::chrono::duration_cast<std::chrono::milliseconds>(
                        std::chrono::system_clock::now().time_since_epoch()) + delay;
                    index.id = id;
                    auto address = &timeoutQueue[writeTimeout];
                    writeTimeout += size;
                    length = size;
                    *timeoutSize = &index.size;
                    return address;
                }
            }
            
            //queue full
            return nullptr;
        }
        
        std::size_t getLevel() const override
        {
            return level;
        }
        
        void incLevel() override
        {
            level++;
        }
        
        void dispatch()
        {
            if (level > 0)
            {
                
                // skip positions when no size fits anymore
                if ((QUEUE_SIZE - (read % QUEUE_SIZE)) < 2)
                {
                    read++;
                }
                
                auto size = *reinterpret_cast<uint32_t*>(&queue[read % QUEUE_SIZE]);
                if (size == 0xFFFFFFFF)
                {
                    read += QUEUE_SIZE - (read % QUEUE_SIZE);
                    size = *reinterpret_cast<uint32_t*>(&queue[read % QUEUE_SIZE]);
                }
                read += sizeof(uint32_t);
                auto m = reinterpret_cast<Message<int>*>(&queue[read % QUEUE_SIZE]);
                m->deliver();
                read += size;

                message::atomic(true);
                level--;
                message::atomic(false);
            }
            checkTimeouts();
        }
        
        /*
         * Defragmentation will only be done when necessary (no further writing possible)
         * timeoutQueueIndex must not be cleaned up since it inserts from fhe beginning
         * into the next free space.
         */
        void defragment()
        {
            // c++ assignment copies
            auto oldTimeoutQueue = timeoutQueue;
            auto oldTimeoutQueueIndex = timeoutQueueIndex;
            
            // clear new table
            for (auto& index : timeoutQueueIndex)
            {
                index.size = 0;
            }
            
            writeTimeout = 0;
            size_t i = 0;
            for (auto& index : oldTimeoutQueueIndex)
            {
                auto address = index.address;
                if (index.size != 0)
                {
                    std::copy(oldTimeoutQueue.begin() + address,
                        oldTimeoutQueue.begin()+address + index.size,
                        timeoutQueue.begin() + writeTimeout);
                    timeoutQueueIndex[i].address = writeTimeout;
                    timeoutQueueIndex[i].size = index.size;
                    timeoutQueueIndex[i].timeout = index.timeout;
                    writeTimeout += index.size;
                    i++;
                }
            }
        }
        
        void checkTimeouts()
        {
            bool defragmentPossible = false;
            for (auto& index : timeoutQueueIndex)
            {
                if ((index.size != 0) && (index.timeout < 
                    std::chrono::duration_cast<std::chrono::milliseconds>(
                    std::chrono::system_clock::now().time_since_epoch())))
                {
                    defragmentPossible = true;
                    auto m = reinterpret_cast<Message<int>*>(&timeoutQueue[index.address]);
                    m->deliver();
                    //disable entry
                    index.size = 0;
                    
                }
            }
            if (defragmentPossible)
            {
                defragment();
            }
        }
        
        void cancelTimeouts(uint32_t id) override
        {
            // id 0 cannot be canceled.
            if (id == 0)
            {
                return;
            }
            
            bool defragmentPossible = false;
            for (auto& index : timeoutQueueIndex)
            {
                if ((index.size != 0) && (index.id == id))
                {
                    defragmentPossible = true;
                    //disable entry
                    index.size = 0;
                    
                }
            }
            if (defragmentPossible)
            {
                defragment();
            }
        }
        
        struct timeoutQueueIndex_t
        {
            uint16_t address = 0;
            uint16_t size = 0;
            uint32_t id = 0;
            std::chrono::milliseconds timeout;
        };
        std::array<uint8_t, QUEUE_SIZE> queue;
        std::array<uint8_t, TIMEOUT_QUEUE_SIZE> timeoutQueue;
        std::array<timeoutQueueIndex_t, TIMEOUT_QUEUE_SIZE / 8> timeoutQueueIndex;
        std::size_t read;
        std::size_t write;
        std::size_t writeTimeout;
        std::size_t level;
    };
}

#endif //__QUEUE_HPP__
