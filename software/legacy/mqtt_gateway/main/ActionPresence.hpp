#pragma once


class ActionPresence : public Action
{
public:
    ActionPresence(ICAN&);
    void init();

private:
    static const char* TAG;
    std::vector<> actions;
};

