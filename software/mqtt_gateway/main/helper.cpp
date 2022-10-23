// std
#include <sstream>

// lib

// local
#include "helper.hpp"

std::string mqtt_split(const char* in, unsigned int len, unsigned int pos)
{
    std::string str(in, len);
    char delim = '/';
    
	size_t start;
	size_t end = 0;
    unsigned int i = 0;
	std::string split;
    while ((start = str.find_first_not_of(delim, end)) != std::string::npos)
	{
		end = str.find(delim, start);
		split = str.substr(start, end - start);
        if (i == pos)
        {
            return split;
        }
        i++;
	}
    split = "";
    return split;
}

std::string toHexString(unsigned int n)
{
    std::stringstream ss;
    ss << std::hex << n;
    return ss.str();
}

unsigned int hextoInt(std::string s)
{
    return std::stoul(s, nullptr, 16);
}
