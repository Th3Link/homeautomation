#pragma once

#include <string>

std::string mqtt_split(const char* in, unsigned int len, unsigned int pos);
std::string toHexString(unsigned int n);
std::string toHexStringPad(unsigned int n);
unsigned int hextoInt(std::string s);
