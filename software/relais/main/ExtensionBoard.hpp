#pragma once

class ExtensionBoard
{
public:
    ExtensionBoard();
    void sensor_board_setup();
    void button_board_setup();
    static const char* TAG;
};
