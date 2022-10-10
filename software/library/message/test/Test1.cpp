#include <iostream>
#include <array>
#include <message/Receiver.hpp>
#include <message/Message.hpp>
#include <message/Queue.hpp>
#include <message/Time.hpp>

class SomeReceiver : public message::Receiver<int>,
                        public message::Receiver<double>,
                        public message::Receiver<std::array<int,5>>
{
public:
    SomeReceiver(message::ReceiverQueue& q) : message::Receiver<int>(q),
        message::Receiver<double>(q),
        message::Receiver<std::array<int,5>>(q) {}
    virtual void receive(message::Message<int>& m) final override
    {
        std::cout << m.data << std::endl;
    }
    virtual void receive(message::Message<double>& m) final override
    {
        std::cout << m.data << std::endl;
    }
    virtual void receive(message::Message<std::array<int,5>>& m) final override
    {
        std::cout << m.data[0] << std::endl;
    }
};

int main(void)
{
    message::Queue<1024, 256> queue;
    
    SomeReceiver rs(queue);
    message::Message<int>::send(rs, message::Event::INIT, 13, std::chrono::seconds(1));
    message::Message<int>::send(rs, message::Event::INIT, 14, std::chrono::seconds(1));
    message::Message<int>::send(rs, message::Event::INIT, 15, std::chrono::seconds(1));
    message::Message<double>::send(rs, message::Event::INIT, 12.88856477, std::chrono::seconds(3));
    message::Message<std::array<int,5>>::send(rs, message::Event::INIT, {1,2,3,4,5}, std::chrono::seconds(8));
    
    while(true)
    {
        queue.dispatch();
    }
    
    return 0;
}

std::chrono::milliseconds message::time()
{
    return std::chrono::duration_cast<std::chrono::milliseconds>(
        std::chrono::system_clock::now().time_since_epoch());
}
