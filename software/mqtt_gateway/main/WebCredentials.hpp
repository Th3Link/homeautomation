#pragma once

class WebCredentials
{
public:
    WebCredentials();
    void init();
    const char* username();
    const char* password();
    void username(const char*);
    void password(const char*);
private:
    static const char* TAG;
    char m_username[30];
    char m_password[30];
};
